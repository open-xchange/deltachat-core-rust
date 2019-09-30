//! End-to-end encryption support.

use std::any::Any;
use std::collections::HashSet;
use std::ffi::CStr;
use std::ptr;
use std::str::FromStr;

use libc::{free, strcmp, strlen, strncmp};
use mmime::clist::*;
use mmime::mailimf::*;
use mmime::mailimf_types::*;
use mmime::mailimf_types_helper::*;
use mmime::mailmime::*;
use mmime::mailmime_content::*;
use mmime::mailmime_types::*;
use mmime::mailmime_types_helper::*;
use mmime::mailmime_write_mem::*;
use mmime::mailprivacy_prepare_mime;
use mmime::mmapstring::*;
use mmime::{mailmime_substitute, MAILIMF_NO_ERROR, MAIL_NO_ERROR};

use crate::aheader::*;
use crate::config::Config;
use crate::context::Context;
use crate::dc_mimeparser::*;
use crate::dc_tools::*;
use crate::error::*;
use crate::key::*;
use crate::keyring::*;
use crate::mimefactory::MimeFactory;
use crate::peerstate::*;
use crate::pgp::*;
use crate::securejoin::handle_degrade_event;
use crate::wrapmime;
use crate::wrapmime::*;

// standard mime-version header aka b"Version: 1\r\n\x00"
static mut VERSION_CONTENT: [libc::c_char; 13] =
    [86, 101, 114, 115, 105, 111, 110, 58, 32, 49, 13, 10, 0];

#[derive(Debug)]
pub struct EncryptHelper {
    pub prefer_encrypt: EncryptPreference,
    pub addr: String,
    pub public_key: Key,
}

impl EncryptHelper {
    pub fn new(context: &Context) -> Result<EncryptHelper> {
        let e2ee = context.sql.get_config_int(&context, "e2ee_enabled");
        let prefer_encrypt = if 0 != e2ee.unwrap_or_default() {
            EncryptPreference::Mutual
        } else {
            EncryptPreference::NoPreference
        };
        let addr = match context.get_config(Config::ConfiguredAddr) {
            None => {
                bail!("addr not configured!");
            }
            Some(addr) => addr,
        };

        let public_key = load_or_generate_self_public_key(context, &addr)?;
        Ok(EncryptHelper {
            prefer_encrypt,
            addr,
            public_key,
        })
    }

    pub fn get_aheader(&self) -> Aheader {
        let pk = self.public_key.clone();
        let addr = self.addr.to_string();
        Aheader::new(addr, pk, self.prefer_encrypt)
    }

    pub fn try_encrypt(
        &mut self,
        factory: &mut MimeFactory,
        e2ee_guaranteed: bool,
        min_verified: libc::c_int,
        do_gossip: bool,
        mut in_out_message: *mut mailmime,
        imffields_unprotected: *mut mailimf_fields,
    ) -> Result<bool> {
        /* libEtPan's pgp_encrypt_mime() takes the parent as the new root.
        We just expect the root as being given to this function. */
        if in_out_message.is_null() || unsafe { !(*in_out_message).mm_parent.is_null() } {
            bail!("corrupted inputs");
        }
        if !(self.prefer_encrypt == EncryptPreference::Mutual || e2ee_guaranteed) {
            return Ok(false);
        }

        let context = &factory.context;
        let mut keyring = Keyring::default();
        let mut gossip_headers: Vec<String> = Vec::with_capacity(factory.recipients_addr.len());

        // determine if we can and should encrypt
        for recipient_addr in factory.recipients_addr.iter() {
            if recipient_addr == &self.addr {
                continue;
            }
            let peerstate = match Peerstate::from_addr(context, &context.sql, recipient_addr) {
                Some(peerstate) => peerstate,
                None => {
                    let msg = format!("peerstate for {} missing, cannot encrypt", recipient_addr);
                    if e2ee_guaranteed {
                        bail!("{}", msg);
                    } else {
                        info!(context, "{}", msg);
                        return Ok(false);
                    }
                }
            };
            if peerstate.prefer_encrypt != EncryptPreference::Mutual && !e2ee_guaranteed {
                info!(context, "peerstate for {} is no-encrypt", recipient_addr);
                return Ok(false);
            }

            if let Some(key) = peerstate.peek_key(min_verified as usize) {
                keyring.add_owned(key.clone());
                if do_gossip {
                    if let Some(header) = peerstate.render_gossip_header(min_verified as usize) {
                        gossip_headers.push(header.to_string());
                    }
                }
            } else {
                bail!(
                    "proper enc-key for {} missing, cannot encrypt",
                    recipient_addr
                );
            }
        }

        let sign_key = {
            keyring.add_ref(&self.public_key);
            let key = Key::from_self_private(context, self.addr.clone(), &context.sql);
            if key.is_none() {
                bail!("no own private key found")
            }
            key
        };

        /* encrypt message */
        unsafe {
            mailprivacy_prepare_mime(in_out_message);
            let mut part_to_encrypt: *mut mailmime =
                (*in_out_message).mm_data.mm_message.mm_msg_mime;
            (*part_to_encrypt).mm_parent = ptr::null_mut();
            let imffields_encrypted: *mut mailimf_fields = mailimf_fields_new_empty();
            /* mailmime_new_message_data() calls mailmime_fields_new_with_version() which would add the unwanted MIME-Version:-header */
            let message_to_encrypt: *mut mailmime = mailmime_new(
                MAILMIME_MESSAGE as libc::c_int,
                ptr::null(),
                0 as libc::size_t,
                mailmime_fields_new_empty(),
                mailmime_get_content_message(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                imffields_encrypted,
                part_to_encrypt,
            );

            for header in &gossip_headers {
                wrapmime::new_custom_field(imffields_encrypted, "Autocrypt-Gossip", &header)
            }

            /* memoryhole headers: move some headers into encrypted part */
            // XXX note we can't use clist's into_iter() because the loop body also removes items
            let mut cur: *mut clistiter = (*(*imffields_unprotected).fld_list).first;
            while !cur.is_null() {
                let field: *mut mailimf_field = (*cur).data as *mut mailimf_field;
                let mut move_to_encrypted = false;
                if !field.is_null() {
                    if (*field).fld_type == MAILIMF_FIELD_SUBJECT as libc::c_int {
                        move_to_encrypted = true;
                    } else if (*field).fld_type == MAILIMF_FIELD_OPTIONAL_FIELD as libc::c_int {
                        let opt_field = (*field).fld_data.fld_optional_field;
                        if !opt_field.is_null() && !(*opt_field).fld_name.is_null() {
                            let fld_name = to_string_lossy((*opt_field).fld_name);
                            if fld_name.starts_with("Secure-Join") || fld_name.starts_with("Chat-")
                            {
                                move_to_encrypted = true;
                            }
                        }
                    }
                }
                if move_to_encrypted {
                    mailimf_fields_add(imffields_encrypted, field);
                    cur = clist_delete((*imffields_unprotected).fld_list, cur);
                } else {
                    cur = (*cur).next;
                }
            }
            let subject: *mut mailimf_subject = mailimf_subject_new("...".strdup());
            mailimf_fields_add(
                imffields_unprotected,
                mailimf_field_new(
                    MAILIMF_FIELD_SUBJECT as libc::c_int,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    subject,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                ),
            );
            wrapmime::append_ct_param(
                (*part_to_encrypt).mm_content_type,
                "protected-headers",
                "v1",
            )?;
            let plain: *mut MMAPString =
                mmap_string_new(b"\x00" as *const u8 as *const libc::c_char);
            let mut col: libc::c_int = 0i32;
            mailmime_write_mem(plain, &mut col, message_to_encrypt);
            mailmime_free(message_to_encrypt);
            if (*plain).str_0.is_null() || (*plain).len <= 0 {
                bail!("could not write/allocate");
            }

            let ctext = dc_pgp_pk_encrypt(
                std::slice::from_raw_parts((*plain).str_0 as *const u8, (*plain).len),
                &keyring,
                sign_key.as_ref(),
            );
            mmap_string_free(plain);

            if let Ok(ctext_v) = ctext {
                /* create MIME-structure that will contain the encrypted text */
                let mut encrypted_part: *mut mailmime = new_data_part(
                    ptr::null_mut(),
                    0 as libc::size_t,
                    "multipart/encrypted",
                    MAILMIME_MECHANISM_BASE64,
                )?;
                let content: *mut mailmime_content = (*encrypted_part).mm_content_type;
                wrapmime::append_ct_param(content, "protocol", "application/pgp-encrypted")?;
                let version_mime: *mut mailmime = new_data_part(
                    VERSION_CONTENT.as_mut_ptr() as *mut libc::c_void,
                    strlen(VERSION_CONTENT.as_mut_ptr()),
                    "application/pgp-encrypted",
                    MAILMIME_MECHANISM_7BIT,
                )?;
                mailmime_smart_add_part(encrypted_part, version_mime);

                // we assume that ctext_v is not dropped until the end
                // of this if-scope
                let ctext_part: *mut mailmime = new_data_part(
                    ctext_v.as_ptr() as *mut libc::c_void,
                    ctext_v.len(),
                    "application/octet-stream",
                    MAILMIME_MECHANISM_7BIT,
                )?;
                mailmime_smart_add_part(encrypted_part, ctext_part);
                (*in_out_message).mm_data.mm_message.mm_msg_mime = encrypted_part;
                (*encrypted_part).mm_parent = in_out_message;
                let gossiped = !&gossip_headers.is_empty();
                factory.finalize_mime_message(in_out_message, true, gossiped)?;
                Ok(true)
            } else {
                bail!("encryption failed")
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct E2eeHelper {
    cdata_to_free: Option<Box<dyn Any>>,

    // for decrypting only
    pub encrypted: bool,
    pub signatures: HashSet<String>,
    pub gossipped_addr: HashSet<String>,
}

impl E2eeHelper {
    /// Frees data referenced by "mailmime" but not freed by mailmime_free(). After calling this function,
    /// in_out_message cannot be used any longer!
    pub unsafe fn thanks(&mut self) {
        if let Some(data) = self.cdata_to_free.take() {
            free(Box::into_raw(data) as *mut _)
        }
    }

    pub unsafe fn decrypt(&mut self, context: &Context, in_out_message: *mut mailmime) {
        /* return values: 0=nothing to decrypt/cannot decrypt, 1=sth. decrypted
        (to detect parts that could not be decrypted, simply look for left "multipart/encrypted" MIME types */
        /*just a pointer into mailmime structure, must not be freed*/
        let imffields: *mut mailimf_fields = mailmime_find_mailimf_fields(in_out_message);
        let mut message_time = 0;
        let mut from = None;
        let mut private_keyring = Keyring::default();
        let mut public_keyring_for_validate = Keyring::default();
        let mut gossip_headers: *mut mailimf_fields = ptr::null_mut();
        if !(in_out_message.is_null() || imffields.is_null()) {
            let mut field = mailimf_find_field(imffields, MAILIMF_FIELD_FROM as libc::c_int);

            if !field.is_null() && !(*field).fld_data.fld_from.is_null() {
                from = mailimf_find_first_addr((*(*field).fld_data.fld_from).frm_mb_list)
            }

            field = mailimf_find_field(imffields, MAILIMF_FIELD_ORIG_DATE as libc::c_int);
            if !field.is_null() && !(*field).fld_data.fld_orig_date.is_null() {
                let orig_date: *mut mailimf_orig_date = (*field).fld_data.fld_orig_date;
                if !orig_date.is_null() {
                    message_time = dc_timestamp_from_date((*orig_date).dt_date_time);
                    if message_time != 0 && message_time > time() {
                        message_time = time()
                    }
                }
            }
            let mut peerstate = None;
            let autocryptheader = from
                .as_ref()
                .and_then(|from| Aheader::from_imffields(from, imffields));
            if message_time > 0 {
                if let Some(ref from) = from {
                    peerstate = Peerstate::from_addr(context, &context.sql, from);

                    if let Some(ref mut peerstate) = peerstate {
                        if let Some(ref header) = autocryptheader {
                            peerstate.apply_header(&header, message_time);
                            peerstate.save_to_db(&context.sql, false);
                        } else if message_time > peerstate.last_seen_autocrypt
                            && !contains_report(in_out_message)
                        {
                            peerstate.degrade_encryption(message_time);
                            peerstate.save_to_db(&context.sql, false);
                        }
                    } else if let Some(ref header) = autocryptheader {
                        let p = Peerstate::from_header(context, header, message_time);
                        assert!(p.save_to_db(&context.sql, true));
                        peerstate = Some(p);
                    }
                }
            }
            /* load private key for decryption */
            let self_addr = context.get_config(Config::ConfiguredAddr);
            if let Some(self_addr) = self_addr {
                if private_keyring.load_self_private_for_decrypting(
                    context,
                    self_addr,
                    &context.sql,
                ) {
                    if peerstate.as_ref().map(|p| p.last_seen).unwrap_or_else(|| 0) == 0 {
                        peerstate =
                            Peerstate::from_addr(&context, &context.sql, &from.unwrap_or_default());
                    }
                    if let Some(ref peerstate) = peerstate {
                        if peerstate.degrade_event.is_some() {
                            handle_degrade_event(context, &peerstate);
                        }
                        if let Some(ref key) = peerstate.gossip_key {
                            public_keyring_for_validate.add_ref(key);
                        }
                        if let Some(ref key) = peerstate.public_key {
                            public_keyring_for_validate.add_ref(key);
                        }
                    }
                    for iterations in 0..10 {
                        let mut has_unencrypted_parts: libc::c_int = 0i32;
                        if decrypt_recursive(
                            context,
                            in_out_message,
                            &private_keyring,
                            &public_keyring_for_validate,
                            &mut self.signatures,
                            &mut gossip_headers,
                            &mut has_unencrypted_parts,
                        )
                        .is_err()
                        {
                            break;
                        }
                        /* if we're here, sth. was encrypted. if we're on top-level,
                        and there are no additional unencrypted parts in the message
                        the encryption was fine (signature is handled separately and
                        returned as `signatures`) */
                        if iterations == 0 && 0 == has_unencrypted_parts {
                            self.encrypted = true;
                        }
                    }
                    /* check for Autocrypt-Gossip */
                    if !gossip_headers.is_null() {
                        self.gossipped_addr = update_gossip_peerstates(
                            context,
                            message_time,
                            imffields,
                            gossip_headers,
                        )
                    }
                }
            }
        }
        //mailmime_print(in_out_message);
        if !gossip_headers.is_null() {
            mailimf_fields_free(gossip_headers);
        }
    }
}

fn new_data_part(
    data: *mut libc::c_void,
    data_bytes: libc::size_t,
    content_type: &str,
    default_encoding: u32,
) -> Result<*mut mailmime> {
    let content = new_content_type(&content_type)?;
    unsafe {
        let mut encoding: *mut mailmime_mechanism = ptr::null_mut();
        if wrapmime::content_type_needs_encoding(content) {
            encoding = mailmime_mechanism_new(default_encoding as i32, ptr::null_mut());
            ensure!(!encoding.is_null(), "failed to create encoding");
        }
        let mime_fields = mailmime_fields_new_with_data(
            encoding,
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
            ptr::null_mut(),
        );
        ensure!(!mime_fields.is_null(), "internal mime error");

        let mime = mailmime_new_empty(content, mime_fields);
        ensure!(!mime.is_null(), "internal mime error");

        if (*mime).mm_type == MAILMIME_SINGLE as libc::c_int {
            if !data.is_null() && data_bytes > 0 {
                mailmime_set_body_text(mime, data as *mut libc::c_char, data_bytes);
            }
        }
        return Ok(mime);
    }
}

/// Load public key from database or generate a new one.
///
/// This will load a public key from the database, generating and
/// storing a new one when one doesn't exist yet.  Care is taken to
/// only generate one key per context even when multiple threads call
/// this function concurrently.
fn load_or_generate_self_public_key(context: &Context, self_addr: impl AsRef<str>) -> Result<Key> {
    if let Some(key) = Key::from_self_public(context, &self_addr, &context.sql) {
        return Ok(key);
    }
    let _guard = context.generating_key_mutex.lock().unwrap();

    // Check again in case the key was generated while we were waiting for the lock.
    if let Some(key) = Key::from_self_public(context, &self_addr, &context.sql) {
        return Ok(key);
    }

    let start = std::time::Instant::now();
    info!(
        context,
        "Generating keypair with {} bits, e={} ...", 2048, 65537,
    );
    match dc_pgp_create_keypair(&self_addr) {
        Some((public_key, private_key)) => {
            match dc_key_save_self_keypair(
                context,
                &public_key,
                &private_key,
                &self_addr,
                1,
                &context.sql,
            ) {
                true => {
                    info!(
                        context,
                        "Keypair generated in {:.3}s.",
                        start.elapsed().as_secs()
                    );
                    Ok(public_key)
                }
                false => Err(format_err!("Failed to save keypair")),
            }
        }
        None => Err(format_err!("Failed to generate keypair")),
    }
}

unsafe fn update_gossip_peerstates(
    context: &Context,
    message_time: i64,
    imffields: *mut mailimf_fields,
    gossip_headers: *const mailimf_fields,
) -> HashSet<String> {
    let mut recipients: Option<HashSet<String>> = None;
    let mut gossipped_addr: HashSet<String> = Default::default();

    for cur_data in (*(*gossip_headers).fld_list).into_iter() {
        let field: *mut mailimf_field = cur_data as *mut _;
        if (*field).fld_type == MAILIMF_FIELD_OPTIONAL_FIELD as libc::c_int {
            let optional_field = (*field).fld_data.fld_optional_field;
            if !optional_field.is_null()
                && !(*optional_field).fld_name.is_null()
                && strcasecmp(
                    (*optional_field).fld_name,
                    b"Autocrypt-Gossip\x00" as *const u8 as *const libc::c_char,
                ) == 0i32
            {
                let value = CStr::from_ptr((*optional_field).fld_value)
                    .to_str()
                    .unwrap();
                let gossip_header = Aheader::from_str(value);
                if let Ok(ref header) = gossip_header {
                    if recipients.is_none() {
                        recipients = Some(mailimf_get_recipients(imffields));
                    }
                    if recipients.as_ref().unwrap().contains(&header.addr) {
                        let mut peerstate =
                            Peerstate::from_addr(context, &context.sql, &header.addr);
                        if let Some(ref mut peerstate) = peerstate {
                            peerstate.apply_gossip(header, message_time);
                            peerstate.save_to_db(&context.sql, false);
                        } else {
                            let p = Peerstate::from_gossip(context, header, message_time);
                            p.save_to_db(&context.sql, true);
                            peerstate = Some(p);
                        }
                        if let Some(peerstate) = peerstate {
                            if peerstate.degrade_event.is_some() {
                                handle_degrade_event(context, &peerstate);
                            }
                        }

                        gossipped_addr.insert(header.addr.clone());
                    } else {
                        info!(
                            context,
                            "Ignoring gossipped \"{}\" as the address is not in To/Cc list.",
                            &header.addr,
                        );
                    }
                }
            }
        }
    }

    gossipped_addr
}

unsafe fn decrypt_recursive(
    context: &Context,
    mime: *mut mailmime,
    private_keyring: &Keyring,
    public_keyring_for_validate: &Keyring,
    ret_valid_signatures: &mut HashSet<String>,
    ret_gossip_headers: *mut *mut mailimf_fields,
    ret_has_unencrypted_parts: *mut libc::c_int,
) -> Result<()> {
    ensure!(!mime.is_null(), "Invalid mime reference");
    let ct: *mut mailmime_content;

    if (*mime).mm_type == MAILMIME_MULTIPLE as libc::c_int {
        ct = (*mime).mm_content_type;
        if !ct.is_null()
            && !(*ct).ct_subtype.is_null()
            && strcmp(
                (*ct).ct_subtype,
                b"encrypted\x00" as *const u8 as *const libc::c_char,
            ) == 0i32
        {
            for cur_data in (*(*mime).mm_data.mm_multipart.mm_mp_list).into_iter() {
                if let Some(decrypted_mime) = decrypt_part(
                    cur_data as *mut mailmime,
                    private_keyring,
                    public_keyring_for_validate,
                    ret_valid_signatures,
                ) {
                    if (*ret_gossip_headers).is_null() && ret_valid_signatures.len() > 0 {
                        let mut dummy: libc::size_t = 0;
                        let mut test: *mut mailimf_fields = ptr::null_mut();
                        if mailimf_envelope_and_optional_fields_parse(
                            (*decrypted_mime).mm_mime_start,
                            (*decrypted_mime).mm_length,
                            &mut dummy,
                            &mut test,
                        ) == MAILIMF_NO_ERROR as libc::c_int
                            && !test.is_null()
                        {
                            *ret_gossip_headers = test
                        }
                    }
                    mailmime_substitute(mime, decrypted_mime);
                    mailmime_free(mime);
                    return Ok(());
                }
            }
            *ret_has_unencrypted_parts = 1i32
        } else {
            for cur_data in (*(*mime).mm_data.mm_multipart.mm_mp_list).into_iter() {
                if decrypt_recursive(
                    context,
                    cur_data as *mut mailmime,
                    private_keyring,
                    public_keyring_for_validate,
                    ret_valid_signatures,
                    ret_gossip_headers,
                    ret_has_unencrypted_parts,
                )
                .is_ok()
                {
                    return Ok(());
                }
            }
        }
    } else if (*mime).mm_type == MAILMIME_MESSAGE as libc::c_int {
        if decrypt_recursive(
            context,
            (*mime).mm_data.mm_message.mm_msg_mime,
            private_keyring,
            public_keyring_for_validate,
            ret_valid_signatures,
            ret_gossip_headers,
            ret_has_unencrypted_parts,
        )
        .is_ok()
        {
            return Ok(());
        }
    } else {
        *ret_has_unencrypted_parts = 1;
    }

    Err(format_err!("Failed to decrypt"))
}

unsafe fn decrypt_part(
    mime: *mut mailmime,
    private_keyring: &Keyring,
    public_keyring_for_validate: &Keyring,
    ret_valid_signatures: &mut HashSet<String>,
) -> Option<*mut mailmime> {
    let mut ok_to_continue = true;
    let mime_data: *mut mailmime_data;
    let mut mime_transfer_encoding: libc::c_int = MAILMIME_MECHANISM_BINARY as libc::c_int;
    /* mmap_string_unref()'d if set */
    let mut transfer_decoding_buffer: *mut libc::c_char = ptr::null_mut();
    /* must not be free()'d */
    let mut decoded_data: *const libc::c_char = ptr::null_mut();
    let mut decoded_data_bytes: libc::size_t = 0;

    let mut res: Option<*mut mailmime> = None;

    mime_data = (*mime).mm_data.mm_single;
    /* MAILMIME_DATA_FILE indicates, the data is in a file; AFAIK this is not used on parsing */
    if !((*mime_data).dt_type != MAILMIME_DATA_TEXT as libc::c_int
        || (*mime_data).dt_data.dt_text.dt_data.is_null()
        || (*mime_data).dt_data.dt_text.dt_length <= 0)
    {
        if !(*mime).mm_mime_fields.is_null() {
            for cur_data in (*(*(*mime).mm_mime_fields).fld_list).into_iter() {
                let field: *mut mailmime_field = cur_data as *mut _;
                if (*field).fld_type == MAILMIME_FIELD_TRANSFER_ENCODING as libc::c_int
                    && !(*field).fld_data.fld_encoding.is_null()
                {
                    mime_transfer_encoding = (*(*field).fld_data.fld_encoding).enc_type
                }
            }
        }
        /* regard `Content-Transfer-Encoding:` */
        if mime_transfer_encoding == MAILMIME_MECHANISM_7BIT as libc::c_int
            || mime_transfer_encoding == MAILMIME_MECHANISM_8BIT as libc::c_int
            || mime_transfer_encoding == MAILMIME_MECHANISM_BINARY as libc::c_int
        {
            decoded_data = (*mime_data).dt_data.dt_text.dt_data;
            decoded_data_bytes = (*mime_data).dt_data.dt_text.dt_length;
            if decoded_data.is_null() || decoded_data_bytes <= 0 {
                /* no error - but no data */
                ok_to_continue = false;
            }
        } else {
            let r: libc::c_int;
            let mut current_index: libc::size_t = 0;
            r = mailmime_part_parse(
                (*mime_data).dt_data.dt_text.dt_data,
                (*mime_data).dt_data.dt_text.dt_length,
                &mut current_index,
                mime_transfer_encoding,
                &mut transfer_decoding_buffer,
                &mut decoded_data_bytes,
            );
            if r != MAILIMF_NO_ERROR as libc::c_int
                || transfer_decoding_buffer.is_null()
                || decoded_data_bytes <= 0
            {
                ok_to_continue = false;
            } else {
                decoded_data = transfer_decoding_buffer;
            }
        }
        if ok_to_continue {
            /* encrypted, decoded data in decoded_data now ... */
            if has_decrypted_pgp_armor(decoded_data, decoded_data_bytes as libc::c_int) {
                let add_signatures = if ret_valid_signatures.is_empty() {
                    Some(ret_valid_signatures)
                } else {
                    None
                };

                /*if we already have fingerprints, do not add more; this ensures, only the fingerprints from the outer-most part are collected */
                if let Ok(plain) = dc_pgp_pk_decrypt(
                    std::slice::from_raw_parts(decoded_data as *const u8, decoded_data_bytes),
                    &private_keyring,
                    &public_keyring_for_validate,
                    add_signatures,
                ) {
                    let plain_bytes = plain.len();
                    let plain_buf = plain.as_ptr() as *const libc::c_char;

                    let mut index: libc::size_t = 0;
                    let mut decrypted_mime: *mut mailmime = ptr::null_mut();
                    if mailmime_parse(
                        plain_buf as *const _,
                        plain_bytes,
                        &mut index,
                        &mut decrypted_mime,
                    ) != MAIL_NO_ERROR as libc::c_int
                        || decrypted_mime.is_null()
                    {
                        if !decrypted_mime.is_null() {
                            mailmime_free(decrypted_mime);
                        }
                    } else {
                        res = Some(decrypted_mime);
                    }
                    std::mem::forget(plain);
                }
            }
        }
    }
    //mailmime_substitute(mime, new_mime);
    //s. mailprivacy_gnupg.c::pgp_decrypt()
    if !transfer_decoding_buffer.is_null() {
        mmap_string_unref(transfer_decoding_buffer);
    }

    res
}

unsafe fn has_decrypted_pgp_armor(str__: *const libc::c_char, mut str_bytes: libc::c_int) -> bool {
    let str_end: *const libc::c_uchar = (str__ as *const libc::c_uchar).offset(str_bytes as isize);
    let mut p: *const libc::c_uchar = str__ as *const libc::c_uchar;
    while p < str_end {
        if *p as libc::c_int > ' ' as i32 {
            break;
        }
        p = p.offset(1isize);
        str_bytes -= 1
    }
    str_bytes > 27i32
        && strncmp(
            p as *const libc::c_char,
            b"-----BEGIN PGP MESSAGE-----\x00" as *const u8 as *const libc::c_char,
            27,
        ) == 0
}

/// Check if a MIME structure contains a multipart/report part.
///
/// As reports are often unencrypted, we do not reset the Autocrypt header in
/// this case.
///
/// However, Delta Chat itself has no problem with encrypted multipart/report
/// parts and MUAs should be encouraged to encrpyt multipart/reports as well so
/// that we could use the normal Autocrypt processing.
unsafe fn contains_report(mime: *mut mailmime) -> bool {
    if (*mime).mm_type == MAILMIME_MULTIPLE as libc::c_int {
        if (*(*(*mime).mm_content_type).ct_type).tp_type
            == MAILMIME_TYPE_COMPOSITE_TYPE as libc::c_int
            && (*(*(*(*mime).mm_content_type).ct_type)
                .tp_data
                .tp_composite_type)
                .ct_type
                == MAILMIME_COMPOSITE_TYPE_MULTIPART as libc::c_int
            && strcmp(
                (*(*mime).mm_content_type).ct_subtype,
                b"report\x00" as *const u8 as *const libc::c_char,
            ) == 0i32
        {
            return true;
        }
        for cur_data in (*(*(*mime).mm_mime_fields).fld_list).into_iter() {
            if contains_report(cur_data as *mut mailmime) {
                return true;
            }
        }
    } else if (*mime).mm_type == MAILMIME_MESSAGE as libc::c_int {
        if contains_report((*mime).mm_data.mm_message.mm_msg_mime) {
            return true;
        }
    }

    false
}

/// Ensures a private key exists for the configured user.
///
/// Normally the private key is generated when the first message is
/// sent but in a few locations there are no such guarantees,
/// e.g. when exporting keys, and calling this function ensures a
/// private key will be present.
///
/// If this succeeds you are also guaranteed that the
/// [Config::ConfiguredAddr] is configured, this address is returned.
pub fn ensure_secret_key_exists(context: &Context) -> Result<String> {
    let self_addr = context
        .get_config(Config::ConfiguredAddr)
        .ok_or(format_err!(concat!(
            "Failed to get self address, ",
            "cannot ensure secret key if not configured."
        )))?;
    load_or_generate_self_public_key(context, &self_addr)?;
    Ok(self_addr)
}

/// Returns the string representation of `mailmime`.
unsafe fn mailmime_to_string(mime: *mut mailmime) -> Result<String> {
    use std::ffi::CString;
    let plain: *mut MMAPString = mmap_string_new(b"\x00" as *const u8 as *const libc::c_char);
    let mut col: libc::c_int = 0i32;
    mailmime_write_mem(plain, &mut col, mime);
    if (*plain).str_0.is_null() || (*plain).len <= 0 {
        bail!("Could not write/allocate");
    }
    let cstr = CString::from_raw((*plain).str_0);
    Ok(cstr.to_str()?.into())
}

fn decrypt_message_from_string(
    context: &Context,
    msg: &str,
    private_keys_for_decryption: &Keyring,
    public_keys: &Keyring,
) -> Result<String> {
    let mut indx: libc::size_t = 0;
    let mut mail: *mut mailmime = ptr::null_mut();
    let res = unsafe { mailmime_parse(msg.as_ptr() as *const i8, msg.len(), &mut indx, &mut mail) };
    if res != 0 {
        bail!("Failed to parse mail");
    }

    let mut valid_signatures: HashSet<String> = HashSet::new();
    let mut gossip_headers: *mut mailimf_fields = ptr::null_mut();
    let mut has_unencrypted_parts: libc::c_int = 0;

    let _ = unsafe {
        decrypt_recursive(
            context,
            mail,
            &private_keys_for_decryption,
            &public_keys,
            &mut valid_signatures,
            &mut gossip_headers,
            &mut has_unencrypted_parts,
        )
    }?;

    if has_unencrypted_parts != 0 {
        bail!("Has unencrypted parts");
    }

    unsafe { mailmime_to_string(mail) }
}

pub fn decrypt_message_in_memory(
    context: &Context,
    content_type: &str,
    content: &str,
    _sender_addr: &str,
) -> Result<String> {
    let full_mime_msg = format!("{}\r\n\r\n{}", content_type, content);

    let self_addr = context
        .sql
        .get_config(context, "configured_addr")
        .unwrap_or_default();

    let mut private_keys_for_decryption = Keyring::default();
    if !private_keys_for_decryption.load_self_private_for_decrypting(
        context,
        self_addr.clone(),
        &context.sql,
    ) {
        bail!("Failed to load private key for decrypting");
    }

    // XXX: Load public key of `_sender_addr` in order to validate signature
    let public_keys = Keyring::default();

    decrypt_message_from_string(
        context,
        &full_mime_msg,
        &private_keys_for_decryption,
        &public_keys,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::test_utils::*;

    mod decryption {
        use super::*;

        #[test]
        fn test_encrypt_and_decrypt() {
            let plain = b"This is a message";

            let t = dummy_context();
            let test_addr = configure_alice_keypair(&t.ctx);

            let mut public_keys_for_encryption = Keyring::default();
            public_keys_for_encryption
                .add_owned(Key::from_self_public(&t.ctx, test_addr.clone(), &t.ctx.sql).unwrap());
            let encrypted_message =
                dc_pgp_pk_encrypt(plain, &public_keys_for_encryption, None).unwrap();

            let mut private_keys_for_decryption = Keyring::default();
            assert!(
                private_keys_for_decryption.load_self_private_for_decrypting(
                    &t.ctx,
                    test_addr.clone(),
                    &t.ctx.sql
                )
            );
            let decrypted_message = dc_pgp_pk_decrypt(
                &encrypted_message.as_bytes(),
                &private_keys_for_decryption,
                &Keyring::default(),
                None,
            )
            .unwrap();
            assert_eq!(decrypted_message, plain);
        }

        #[test]
        fn test_decrypt_message() {
            let encrypted_message = "-----BEGIN PGP MESSAGE-----

wcBMA5Og3DZG63HoAQf/V375OzDFEbvqaO19mPWnB4rc+jA2E0b4NaxIWnLVQZpL
/kb4MH0tbh8EDHhFs3IL8LD6o7Y/pkwZnHZ9va5zm+75vRMXKCSsaqCXhu4yYQL7
JdwSua1byr0pYXGU4Trz6Yrga1sv49I1PAlj1StEYCOaK+vYaYG/EAPwrU/szgIL
Iq0oIf3wySlAgRXfbYwgcuem7JbOUJZtqlwNxekkO2g2A5M0geOuufIw9dvevBqx
gULxeS72mLkJkpgOzckaDV9K/6F3lhO7z7qOdb/c2K3FOmQPF7OCFTLqaCMFGiEv
mCDjB2u7+JHfBeH3sXNu55d3qlltseG2cAEbnS3j69JCAVq0UzMidVWwiX+0Z/Li
Ju7oJPGwXBqe/XPDD9NojzYmHG3uVgyFALTgXRkSOk8y/wKVvSaAZLhETV3sIa0r
QgoI
=iQ+M
-----END PGP MESSAGE-----
";
            let t = dummy_context();
            let test_addr = configure_alice_keypair(&t.ctx);
            let mut private_keys_for_decryption = Keyring::default();
            assert!(
                private_keys_for_decryption.load_self_private_for_decrypting(
                    &t.ctx,
                    test_addr.clone(),
                    &t.ctx.sql
                )
            );
            let decrypted_message = dc_pgp_pk_decrypt(
                &encrypted_message.as_bytes(),
                &private_keys_for_decryption,
                &Keyring::default(),
                None,
            )
            .unwrap();
            assert_eq!(decrypted_message, b"This is a message");
        }

        #[test]
        fn test_decrypt_message_in_memory() {
            let content_type = r###"Content-Type: multipart/encrypted; boundary="5d8b0f2e_f8f75182_bb0c"; protocol="application/pgp-encrypted"###;
            let content = r###"--5d8b0f2e_f8f75182_bb0c
Content-Type: application/pgp-encrypted
Content-Transfer-Encoding: 7bit

Version: 1

--5d8b0f2e_f8f75182_bb0c
Content-Type: application/octet-stream
Content-Transfer-Encoding: 7bit

-----BEGIN PGP MESSAGE-----

wcBMA5Og3DZG63HoAQf/V375OzDFEbvqaO19mPWnB4rc+jA2E0b4NaxIWnLVQZpL
/kb4MH0tbh8EDHhFs3IL8LD6o7Y/pkwZnHZ9va5zm+75vRMXKCSsaqCXhu4yYQL7
JdwSua1byr0pYXGU4Trz6Yrga1sv49I1PAlj1StEYCOaK+vYaYG/EAPwrU/szgIL
Iq0oIf3wySlAgRXfbYwgcuem7JbOUJZtqlwNxekkO2g2A5M0geOuufIw9dvevBqx
gULxeS72mLkJkpgOzckaDV9K/6F3lhO7z7qOdb/c2K3FOmQPF7OCFTLqaCMFGiEv
mCDjB2u7+JHfBeH3sXNu55d3qlltseG2cAEbnS3j69JCAVq0UzMidVWwiX+0Z/Li
Ju7oJPGwXBqe/XPDD9NojzYmHG3uVgyFALTgXRkSOk8y/wKVvSaAZLhETV3sIa0r
QgoI
=iQ+M
-----END PGP MESSAGE-----

--5d8b0f2e_f8f75182_bb0c--

"###;
            let sender_addr = "bob@example.org";
            let expected_decrypted_msg = "Content-Type: message/rfc822\r\n\r\nContent-Type: text/plain\r\n\r\nThis is a message";

            let t = dummy_context();
            let _ = configure_alice_keypair(&t.ctx);

            assert_eq!(
                expected_decrypted_msg,
                decrypt_message_in_memory(&t.ctx, content_type, content, sender_addr).unwrap()
            );
        }

        #[test]
        fn test_decrypt_message_from_string() {
            let msg = r###"Content-Type: multipart/encrypted; boundary="5d8b0f2e_f8f75182_bb0c"; protocol="application/pgp-encrypted";

--5d8b0f2e_f8f75182_bb0c
Content-Type: application/pgp-encrypted
Content-Transfer-Encoding: 7bit

Version: 1

--5d8b0f2e_f8f75182_bb0c
Content-Type: application/octet-stream
Content-Transfer-Encoding: 7bit

-----BEGIN PGP MESSAGE-----

wcBMA5Og3DZG63HoAQf/V375OzDFEbvqaO19mPWnB4rc+jA2E0b4NaxIWnLVQZpL
/kb4MH0tbh8EDHhFs3IL8LD6o7Y/pkwZnHZ9va5zm+75vRMXKCSsaqCXhu4yYQL7
JdwSua1byr0pYXGU4Trz6Yrga1sv49I1PAlj1StEYCOaK+vYaYG/EAPwrU/szgIL
Iq0oIf3wySlAgRXfbYwgcuem7JbOUJZtqlwNxekkO2g2A5M0geOuufIw9dvevBqx
gULxeS72mLkJkpgOzckaDV9K/6F3lhO7z7qOdb/c2K3FOmQPF7OCFTLqaCMFGiEv
mCDjB2u7+JHfBeH3sXNu55d3qlltseG2cAEbnS3j69JCAVq0UzMidVWwiX+0Z/Li
Ju7oJPGwXBqe/XPDD9NojzYmHG3uVgyFALTgXRkSOk8y/wKVvSaAZLhETV3sIa0r
QgoI
=iQ+M
-----END PGP MESSAGE-----

--5d8b0f2e_f8f75182_bb0c--

"###;
            let expected_decrypted_msg = "Content-Type: message/rfc822\r\n\r\nContent-Type: text/plain\r\n\r\nThis is a message";

            let t = dummy_context();
            let _ = configure_alice_keypair(&t.ctx);

            let self_addr = t
                .ctx
                .sql
                .get_config(&t.ctx, "configured_addr")
                .unwrap_or_default();

            let mut private_keys_for_decryption = Keyring::default();
            assert!(
                private_keys_for_decryption.load_self_private_for_decrypting(
                    &t.ctx,
                    self_addr.clone(),
                    &t.ctx.sql
                )
            );
            let public_keys = Keyring::default();

            assert_eq!(
                expected_decrypted_msg,
                decrypt_message_from_string(
                    &t.ctx,
                    msg,
                    &private_keys_for_decryption,
                    &public_keys
                )
                .unwrap()
            );
        }
    }

    mod ensure_secret_key_exists {
        use super::*;

        #[test]
        fn test_prexisting() {
            let t = dummy_context();
            let test_addr = configure_alice_keypair(&t.ctx);
            assert_eq!(ensure_secret_key_exists(&t.ctx).unwrap(), test_addr);
        }

        #[test]
        fn test_not_configured() {
            let t = dummy_context();
            assert!(ensure_secret_key_exists(&t.ctx).is_err());
        }
    }

    #[test]
    fn test_mailmime_parse() {
        let plain = b"Chat-Disposition-Notification-To: holger@deltachat.de
Chat-Group-ID: CovhGgau8M-
Chat-Group-Name: Delta Chat Dev
Subject: =?utf-8?Q?Chat=3A?= Delta Chat =?utf-8?Q?Dev=3A?= sidenote for
 =?utf-8?Q?all=3A?= rust core master ...
Content-Type: text/plain; charset=\"utf-8\"; protected-headers=\"v1\"
Content-Transfer-Encoding: quoted-printable

sidenote for all: rust core master is broken currently ... so dont recomm=
end to try to run with desktop or ios unless you are ready to hunt bugs

-- =20
Sent with my Delta Chat Messenger: https://delta.chat";
        let plain_bytes = plain.len();
        let plain_buf = plain.as_ptr() as *const libc::c_char;

        let mut index = 0;
        let mut decrypted_mime = std::ptr::null_mut();

        let res = unsafe {
            mailmime_parse(
                plain_buf as *const _,
                plain_bytes,
                &mut index,
                &mut decrypted_mime,
            )
        };
        unsafe {
            let msg1 = (*decrypted_mime).mm_data.mm_message.mm_msg_mime;
            let data = mailmime_transfer_decode(msg1).unwrap();
            println!("{:?}", String::from_utf8_lossy(&data));
        }

        assert_eq!(res, 0);
        assert!(!decrypted_mime.is_null());

        unsafe { free(decrypted_mime as *mut _) };
    }

    mod load_or_generate_self_public_key {
        use super::*;

        #[test]
        fn test_existing() {
            let t = dummy_context();
            let addr = configure_alice_keypair(&t.ctx);
            let key = load_or_generate_self_public_key(&t.ctx, addr);
            assert!(key.is_ok());
        }

        #[test]
        #[ignore] // generating keys is expensive
        fn test_generate() {
            let t = dummy_context();
            let addr = "alice@example.org";
            let key0 = load_or_generate_self_public_key(&t.ctx, addr);
            assert!(key0.is_ok());
            let key1 = load_or_generate_self_public_key(&t.ctx, addr);
            assert!(key1.is_ok());
            assert_eq!(key0.unwrap(), key1.unwrap());
        }

        #[test]
        #[ignore]
        fn test_generate_concurrent() {
            use std::sync::Arc;
            use std::thread;

            let t = dummy_context();
            let ctx = Arc::new(t.ctx);
            let ctx0 = Arc::clone(&ctx);
            let thr0 =
                thread::spawn(move || load_or_generate_self_public_key(&ctx0, "alice@example.org"));
            let ctx1 = Arc::clone(&ctx);
            let thr1 =
                thread::spawn(move || load_or_generate_self_public_key(&ctx1, "alice@example.org"));
            let res0 = thr0.join().unwrap();
            let res1 = thr1.join().unwrap();
            assert_eq!(res0.unwrap(), res1.unwrap());
        }
    }
}
