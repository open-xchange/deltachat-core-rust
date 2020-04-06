//! Some tools and enhancements to the used libraries, there should be
//! no references to Context and other "larger" entities here.

use core::cmp::{max, min};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::SystemTime;
use std::{fmt, fs};

use chrono::{Local, TimeZone};
use rand::{thread_rng, Rng};

use crate::config::Config;
use crate::context::Context;
use crate::error::Error;
use crate::events::Event;

pub(crate) fn dc_exactly_one_bit_set(v: i32) -> bool {
    0 != v && 0 == v & (v - 1)
}

/// Shortens a string to a specified length and adds "[...]" to the
/// end of the shortened string.
pub(crate) fn dc_truncate(buf: &str, approx_chars: usize) -> Cow<str> {
    let ellipse = "[...]";

    let count = buf.chars().count();
    if approx_chars > 0 && count > approx_chars + ellipse.len() {
        let end_pos = buf
            .char_indices()
            .nth(approx_chars)
            .map(|(n, _)| n)
            .unwrap_or_default();

        if let Some(index) = buf[..end_pos].rfind(|c| c == ' ' || c == '\n') {
            Cow::Owned(format!("{}{}", &buf[..=index], ellipse))
        } else {
            Cow::Owned(format!("{}{}", &buf[..end_pos], ellipse))
        }
    } else {
        Cow::Borrowed(buf)
    }
}

/// the colors must fulfill some criterions as:
/// - contrast to black and to white
/// - work as a text-color
/// - being noticeable on a typical map
/// - harmonize together while being different enough
/// (therefore, we cannot just use random rgb colors :)
const COLORS: [u32; 16] = [
    0xe5_65_55, 0xf2_8c_48, 0x8e_85_ee, 0x76_c8_4d, 0x5b_b6_cc, 0x54_9c_dd, 0xd2_5c_99, 0xb3_78_00,
    0xf2_30_30, 0x39_b2_49, 0xbb_24_3b, 0x96_40_78, 0x66_87_4f, 0x30_8a_b9, 0x12_7e_d0, 0xbe_45_0c,
];

pub(crate) fn dc_str_to_color(s: impl AsRef<str>) -> u32 {
    let str_lower = s.as_ref().to_lowercase();
    let mut checksum = 0;
    let bytes = str_lower.as_bytes();
    for (i, byte) in bytes.iter().enumerate() {
        checksum += (i + 1) * *byte as usize;
        checksum %= 0x00ff_ffff;
    }
    let color_index = checksum % COLORS.len();

    COLORS[color_index]
}

/* ******************************************************************************
 * date/time tools
 ******************************************************************************/

pub fn dc_timestamp_to_str(wanted: i64) -> String {
    let ts = Local.timestamp(wanted, 0);
    ts.format("%Y.%m.%d %H:%M:%S").to_string()
}

pub(crate) fn dc_gm2local_offset() -> i64 {
    /* returns the offset that must be _added_ to an UTC/GMT-time to create the localtime.
    the function may return negative values. */
    let lt = Local::now();
    lt.offset().local_minus_utc() as i64
}

// timesmearing
// - as e-mails typically only use a second-based-resolution for timestamps,
//   the order of two mails sent withing one second is unclear.
//   this is bad eg. when forwarding some messages from a chat -
//   these messages will appear at the recipient easily out of order.
// - we work around this issue by not sending out two mails with the same timestamp.
// - for this purpose, in short, we track the last timestamp used in `last_smeared_timestamp`
//   when another timestamp is needed in the same second, we use `last_smeared_timestamp+1`
// - after some moments without messages sent out,
//   `last_smeared_timestamp` is again in sync with the normal time.
// - however, we do not do all this for the far future,
//   but at max `MAX_SECONDS_TO_LEND_FROM_FUTURE`
const MAX_SECONDS_TO_LEND_FROM_FUTURE: i64 = 5;

// returns the currently smeared timestamp,
// may be used to check if call to dc_create_smeared_timestamp() is needed or not.
// the returned timestamp MUST NOT be used to be sent out or saved in the database!
pub(crate) fn dc_smeared_time(context: &Context) -> i64 {
    let mut now = time();
    let ts = *context.last_smeared_timestamp.read().unwrap();
    if ts >= now {
        now = ts + 1;
    }

    now
}

// returns a timestamp that is guaranteed to be unique.
pub(crate) fn dc_create_smeared_timestamp(context: &Context) -> i64 {
    let now = time();
    let mut ret = now;

    let mut last_smeared_timestamp = context.last_smeared_timestamp.write().unwrap();
    if ret <= *last_smeared_timestamp {
        ret = *last_smeared_timestamp + 1;
        if ret - now > MAX_SECONDS_TO_LEND_FROM_FUTURE {
            ret = now + MAX_SECONDS_TO_LEND_FROM_FUTURE
        }
    }

    *last_smeared_timestamp = ret;
    ret
}

// creates `count` timestamps that are guaranteed to be unique.
// the frist created timestamps is returned directly,
// get the other timestamps just by adding 1..count-1
pub(crate) fn dc_create_smeared_timestamps(context: &Context, count: usize) -> i64 {
    let now = time();
    let count = count as i64;
    let mut start = now + min(count, MAX_SECONDS_TO_LEND_FROM_FUTURE) - count;

    let mut last_smeared_timestamp = context.last_smeared_timestamp.write().unwrap();
    start = max(*last_smeared_timestamp + 1, start);

    *last_smeared_timestamp = start + count - 1;
    start
}

/* Message-ID tools */
pub(crate) fn dc_create_id() -> String {
    /* generate an id. the generated ID should be as short and as unique as possible:
    - short, because it may also used as part of Message-ID headers or in QR codes
    - unique as two IDs generated on two devices should not be the same. However, collisions are not world-wide but only by the few contacts.
    IDs generated by this function are 66 bit wide and are returned as 11 base64 characters.
    If possible, RNG of OpenSSL is used.

    Additional information when used as a message-id or group-id:
    - for OUTGOING messages this ID is written to the header as `Chat-Group-ID:` and is added to the message ID as Gr.<grpid>.<random>@<random>
    - for INCOMING messages, the ID is taken from the Chat-Group-ID-header or from the Message-ID in the In-Reply-To: or References:-Header
    - the group-id should be a string with the characters [a-zA-Z0-9\-_] */

    let mut rng = thread_rng();
    let buf: [u32; 3] = [rng.gen(), rng.gen(), rng.gen()];

    encode_66bits_as_base64(buf[0usize], buf[1usize], buf[2usize])
}

/// Encode 66 bits as a base64 string.
/// This is useful for ID generating with short strings as we save 5 character
/// in each id compared to 64 bit hex encoding. For a typical group ID, these
/// are 10 characters (grpid+msgid):
///    hex:    64 bit, 4 bits/character, length = 64/4 = 16 characters
///    base64: 64 bit, 6 bits/character, length = 64/6 = 11 characters (plus 2 additional bits)
/// Only the lower 2 bits of `fill` are used.
fn encode_66bits_as_base64(v1: u32, v2: u32, fill: u32) -> String {
    use byteorder::{BigEndian, WriteBytesExt};

    let mut wrapped_writer = Vec::new();
    {
        let mut enc = base64::write::EncoderWriter::new(&mut wrapped_writer, base64::URL_SAFE);
        enc.write_u32::<BigEndian>(v1).unwrap();
        enc.write_u32::<BigEndian>(v2).unwrap();
        enc.write_u8(((fill & 0x3) as u8) << 6).unwrap();
        enc.finish().unwrap();
    }
    assert_eq!(wrapped_writer.pop(), Some(b'A')); // Remove last "A"
    String::from_utf8(wrapped_writer).unwrap()
}

/// Function generates a Message-ID that can be used for a new outgoing message.
/// - this function is called for all outgoing messages.
/// - the message ID should be globally unique
/// - do not add a counter or any private data as this leaks information unncessarily
pub fn dc_create_outgoing_rfc724_mid(
    context: &Context,
    group_id: Option<&str>,
    from_addr: &str,
) -> String {
    let at_hostname = from_addr
        .find('@')
        .map(|k| &from_addr[k..])
        .unwrap_or("@nohost");

    match group_id {
        None => {
            let mid_prefix = if context.get_config_bool(Config::Rfc724MsgIdPrefix) {
                "chat$"
            } else {
                "Mr."
            };
            format!(
                "{mid_prefix}{group}.{rand2}{hostname}",
                mid_prefix = mid_prefix,
                group = dc_create_id(),
                rand2 = dc_create_id(),
                hostname = at_hostname
            )
        }
        Some(group) => {
            let mid_prefix = if context.get_config_bool(Config::Rfc724MsgIdPrefix) {
                "chat$group."
            } else {
                "Gr."
            };
            format!(
                "{mid_prefix}{group}.{rand2}{hostname}",
                mid_prefix = mid_prefix,
                group = group,
                rand2 = dc_create_id(),
                hostname = at_hostname
            )
        }
    }
}

/// Strip off `prefix` from `s` and return the remaining string if `s` starts with `prefix`.  If
/// string `s` does not start with `prefix`, return None.
///
/// # Examples
/// ```rust
/// use deltachat::dc_tools::split_prefix;
///
/// let grp_id = "Gr.12345678901.morerandom@domain.de";
/// assert_eq!(split_prefix(grp_id, "Gr."), Some("12345678901.morerandom@domain.de"));
/// assert_eq!(split_prefix(grp_id, "chat$group."), None);
/// ```
pub fn split_prefix<'a>(s: &'a str, prefix: &str) -> Option<&'a str> {
    if s.starts_with(prefix) {
        s.get(prefix.len()..)
    } else {
        None
    }
}

#[test]
fn test_split_prefix() {
    assert_eq!(split_prefix("", "Gr."), None);
    assert_eq!(split_prefix("Gr", "Gr."), None);
    assert_eq!(split_prefix("_Gr.", "Gr."), None);
    assert_eq!(split_prefix("Gr.", "Gr."), Some(""));
    assert_eq!(split_prefix("Gr.following", "Gr."), Some("following"));
    assert_eq!(split_prefix("Gr.Gr.", "Gr."), Some("Gr."));
}

const OLD_GROUP_ID_PREFIX: &str = "Gr.";
const NEW_GROUP_ID_PREFIX: &str = "chat$group.";

/// Extract the group id (grpid) from a message id (mid)
///
/// # Arguments
///
/// * `mid` - A string that holds the message id.  Leading/Trailing <>
/// characters are automatically stripped.
pub(crate) fn dc_extract_grpid_from_rfc724_mid(mid: &str) -> Option<&str> {
    let mid = mid.trim_start_matches('<').trim_end_matches('>');

    if mid.len() < 9 || !mid.starts_with(OLD_GROUP_ID_PREFIX) && !mid.starts_with(NEW_GROUP_ID_PREFIX) {
        println!("### returning for mid: '{}'", mid);
        return None;
    }

    if let Some(suffix) = split_prefix(mid, NEW_GROUP_ID_PREFIX) {
        return suffix
            .split('.')
            .next()
            .filter(|new_grpid| new_grpid.len() >= 11 || new_grpid.len() == 16);
    }

    if let Some(suffix) = split_prefix(mid, OLD_GROUP_ID_PREFIX) {
        return suffix
            .split('.')
            .next()
            .filter(|old_grpid| old_grpid.len() == 11 || old_grpid.len() == 16);
    }

    None
}

// the returned suffix is lower-case
pub fn dc_get_filesuffix_lc(path_filename: impl AsRef<str>) -> Option<String> {
    Path::new(path_filename.as_ref())
        .extension()
        .map(|p| p.to_string_lossy().to_lowercase())
}

/// Returns the `(width, height)` of the given image buffer.
pub fn dc_get_filemeta(buf: &[u8]) -> Result<(u32, u32), Error> {
    let meta = image_meta::load_from_buf(buf)?;

    Ok((meta.dimensions.width, meta.dimensions.height))
}

/// Expand paths relative to $BLOBDIR into absolute paths.
///
/// If `path` starts with "$BLOBDIR", replaces it with the blobdir path.
/// Otherwise, returns path as is.
pub(crate) fn dc_get_abs_path<P: AsRef<std::path::Path>>(
    context: &Context,
    path: P,
) -> std::path::PathBuf {
    let p: &std::path::Path = path.as_ref();
    if let Ok(p) = p.strip_prefix("$BLOBDIR") {
        context.get_blobdir().join(p)
    } else {
        p.into()
    }
}

pub(crate) fn dc_get_filebytes(context: &Context, path: impl AsRef<std::path::Path>) -> u64 {
    let path_abs = dc_get_abs_path(context, &path);
    match fs::metadata(&path_abs) {
        Ok(meta) => meta.len() as u64,
        Err(_err) => 0,
    }
}

pub(crate) fn dc_delete_file(context: &Context, path: impl AsRef<std::path::Path>) -> bool {
    let path_abs = dc_get_abs_path(context, &path);
    if !path_abs.exists() {
        return false;
    }
    if !path_abs.is_file() {
        warn!(
            context,
            "refusing to delete non-file \"{}\".",
            path.as_ref().display()
        );
        return false;
    }

    let dpath = format!("{}", path.as_ref().to_string_lossy());
    match fs::remove_file(path_abs) {
        Ok(_) => {
            context.call_cb(Event::DeletedBlobFile(dpath));
            true
        }
        Err(err) => {
            warn!(context, "Cannot delete \"{}\": {}", dpath, err);
            false
        }
    }
}

pub(crate) fn dc_copy_file(
    context: &Context,
    src_path: impl AsRef<std::path::Path>,
    dest_path: impl AsRef<std::path::Path>,
) -> bool {
    let src_abs = dc_get_abs_path(context, &src_path);
    let mut src_file = match fs::File::open(&src_abs) {
        Ok(file) => file,
        Err(err) => {
            warn!(
                context,
                "failed to open for read '{}': {}",
                src_abs.display(),
                err
            );
            return false;
        }
    };

    let dest_abs = dc_get_abs_path(context, &dest_path);
    let mut dest_file = match fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&dest_abs)
    {
        Ok(file) => file,
        Err(err) => {
            warn!(
                context,
                "failed to open for write '{}': {}",
                dest_abs.display(),
                err
            );
            return false;
        }
    };

    match std::io::copy(&mut src_file, &mut dest_file) {
        Ok(_) => true,
        Err(err) => {
            error!(
                context,
                "Cannot copy \"{}\" to \"{}\": {}",
                src_abs.display(),
                dest_abs.display(),
                err
            );
            {
                // Attempt to remove the failed file, swallow errors resulting from that.
                fs::remove_file(dest_abs).ok();
            }
            false
        }
    }
}

pub(crate) fn dc_create_folder(
    context: &Context,
    path: impl AsRef<std::path::Path>,
) -> Result<(), std::io::Error> {
    let path_abs = dc_get_abs_path(context, &path);
    if !path_abs.exists() {
        match fs::create_dir_all(path_abs) {
            Ok(_) => Ok(()),
            Err(err) => {
                warn!(
                    context,
                    "Cannot create directory \"{}\": {}",
                    path.as_ref().display(),
                    err
                );
                Err(err)
            }
        }
    } else {
        Ok(())
    }
}

/// Write a the given content to provied file path.
pub(crate) fn dc_write_file(
    context: &Context,
    path: impl AsRef<Path>,
    buf: &[u8],
) -> Result<(), std::io::Error> {
    let path_abs = dc_get_abs_path(context, &path);
    fs::write(&path_abs, buf).map_err(|err| {
        warn!(
            context,
            "Cannot write {} bytes to \"{}\": {}",
            buf.len(),
            path.as_ref().display(),
            err
        );
        err
    })
}

pub fn dc_read_file<P: AsRef<std::path::Path>>(
    context: &Context,
    path: P,
) -> Result<Vec<u8>, Error> {
    let path_abs = dc_get_abs_path(context, &path);

    match fs::read(&path_abs) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            warn!(
                context,
                "Cannot read \"{}\" or file is empty: {}",
                path.as_ref().display(),
                err
            );
            Err(err.into())
        }
    }
}

pub fn dc_open_file<P: AsRef<std::path::Path>>(
    context: &Context,
    path: P,
) -> Result<std::fs::File, Error> {
    let path_abs = dc_get_abs_path(context, &path);

    match fs::File::open(&path_abs) {
        Ok(bytes) => Ok(bytes),
        Err(err) => {
            warn!(
                context,
                "Cannot read \"{}\" or file is empty: {}",
                path.as_ref().display(),
                err
            );
            Err(err.into())
        }
    }
}

pub(crate) fn dc_get_next_backup_path(
    folder: impl AsRef<Path>,
    backup_time: i64,
) -> Result<PathBuf, Error> {
    let folder = PathBuf::from(folder.as_ref());
    let stem = chrono::NaiveDateTime::from_timestamp(backup_time, 0)
        .format("delta-chat-%Y-%m-%d")
        .to_string();

    // 64 backup files per day should be enough for everyone
    for i in 0..64 {
        let mut path = folder.clone();
        path.push(format!("{}-{}.bak", stem, i));
        if !path.exists() {
            return Ok(path);
        }
    }
    bail!("could not create backup file, disk full?");
}

pub(crate) fn time() -> i64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Very simple email address wrapper.
///
/// Represents an email address, right now just the `name@domain` portion.
///
/// # Example
///
/// ```
/// use deltachat::dc_tools::EmailAddress;
/// let email = match EmailAddress::new("someone@example.com") {
///     Ok(addr) => addr,
///     Err(e) => panic!("Error parsing address, error was {}", e),
/// };
/// assert_eq!(&email.local, "someone");
/// assert_eq!(&email.domain, "example.com");
/// assert_eq!(email.to_string(), "someone@example.com");
/// ```
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct EmailAddress {
    pub local: String,
    pub domain: String,
}

impl EmailAddress {
    pub fn new(input: &str) -> Result<Self, Error> {
        input.parse::<EmailAddress>()
    }
}

impl fmt::Display for EmailAddress {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}@{}", self.local, self.domain)
    }
}

impl FromStr for EmailAddress {
    type Err = Error;

    /// Performs a dead-simple parse of an email address.
    fn from_str(input: &str) -> Result<EmailAddress, Error> {
        ensure!(!input.is_empty(), "empty string is not valid");
        let parts: Vec<&str> = input.rsplitn(2, '@').collect();

        ensure!(parts.len() > 1, "missing '@' character");
        let local = parts[1];
        let domain = parts[0];

        ensure!(
            !local.is_empty(),
            "empty string is not valid for local part"
        );
        ensure!(domain.len() > 3, "domain is too short");

        let dot = domain.find('.');
        ensure!(dot.is_some(), "invalid domain");
        ensure!(dot.unwrap() < domain.len() - 2, "invalid domain");

        Ok(EmailAddress {
            local: local.to_string(),
            domain: domain.to_string(),
        })
    }
}

/// Utility to check if a in the binary represantion of listflags
/// the bit at position bitindex is 1.
pub(crate) fn listflags_has(listflags: u32, bitindex: usize) -> bool {
    let listflags = listflags as usize;
    (listflags & bitindex) == bitindex
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::TryInto;

    use crate::constants::*;
    use crate::test_utils::*;

    #[test]
    fn test_rust_ftoa() {
        assert_eq!("1.22", format!("{}", 1.22));
    }

    #[test]
    fn test_dc_truncate_1() {
        let s = "this is a little test string";
        assert_eq!(dc_truncate(s, 16), "this is a [...]");
    }

    #[test]
    fn test_dc_truncate_2() {
        assert_eq!(dc_truncate("1234", 2), "1234");
    }

    #[test]
    fn test_dc_truncate_3() {
        assert_eq!(dc_truncate("1234567", 1), "1[...]");
    }

    #[test]
    fn test_dc_truncate_4() {
        assert_eq!(dc_truncate("123456", 4), "123456");
    }

    #[test]
    fn test_dc_truncate_edge() {
        assert_eq!(dc_truncate("", 4), "");

        assert_eq!(dc_truncate("\n  hello \n world", 4), "\n  [...]");

        assert_eq!(dc_truncate("𐠈0Aᝮa𫝀®!ꫛa¡0A𐢧00𐹠®A  丽ⷐએ", 1), "𐠈[...]");
        assert_eq!(
            dc_truncate("𐠈0Aᝮa𫝀®!ꫛa¡0A𐢧00𐹠®A  丽ⷐએ", 0),
            "𐠈0Aᝮa𫝀®!ꫛa¡0A𐢧00𐹠®A  丽ⷐએ"
        );

        // 9 characters, so no truncation
        assert_eq!(dc_truncate("𑒀ὐ￠🜀\u{1e01b}A a🟠", 6), "𑒀ὐ￠🜀\u{1e01b}A a🟠",);

        // 12 characters, truncation
        assert_eq!(
            dc_truncate("𑒀ὐ￠🜀\u{1e01b}A a🟠bcd", 6),
            "𑒀ὐ￠🜀\u{1e01b}A[...]",
        );
    }

    #[test]
    fn test_dc_create_id() {
        let buf = dc_create_id();
        assert_eq!(buf.len(), 11);
    }

    #[test]
    fn test_encode_66bits_as_base64() {
        assert_eq!(
            encode_66bits_as_base64(0x01234567, 0x89abcdef, 0),
            "ASNFZ4mrze8"
        );
        assert_eq!(
            encode_66bits_as_base64(0x01234567, 0x89abcdef, 1),
            "ASNFZ4mrze9"
        );
        assert_eq!(
            encode_66bits_as_base64(0x01234567, 0x89abcdef, 2),
            "ASNFZ4mrze-"
        );
        assert_eq!(
            encode_66bits_as_base64(0x01234567, 0x89abcdef, 3),
            "ASNFZ4mrze_"
        );
    }

    #[test]
    fn test_dc_extract_grpid_from_rfc724_mid() {
        // Should return None if we pass invalid mid
        let mid = "foobar";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
    }

    #[test]
    fn test_dc_extract_grpid_from_rfc724_mid_with_unknown_prefix() {
        let mid = "foo.12345678901.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
        let mid = "foo$123456789012.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
        let mid = "foo.1234567890123456.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
    }

    #[test]
    fn test_dc_extract_old_grpid_from_rfc724_mid_with_too_short_grpid() {
        let mid = "Gr.12345678.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
    }

    #[test]
    fn test_dc_extract_old_grpid_from_rfc724_mid_with_acceptable_len11_grpid() {
        let mid = "Gr.12345678901.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, Some("12345678901"));
    }

    #[test]
    fn test_dc_extract_old_grpid_from_rfc724_mid_with_wrongsized_len12_grpid() {
        let mid = "Gr.123456789012.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
    }

    #[test]
    fn test_dc_extract_old_grpid_from_rfc724_mid_with_acceptable_len16_grpid() {
        let mid = "Gr.1234567890123456.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, Some("1234567890123456"));

        // Should return extracted grpid for grpid with length of 11
        let mid = "<Gr.12345678901.morerandom@domain.de>";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, Some("12345678901"));

        // Should return extracted grpid for grpid with length of 11
        let mid = "<Gr.1234567890123456.morerandom@domain.de>";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, Some("1234567890123456"));
    }

    #[test]
    fn test_dc_extract_new_grpid_from_rfc724_mid_with_too_short_grpid() {
        let mid = "chat$group.12345678901.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, None);
    }

    #[test]
    fn test_dc_extract_new_grpid_from_rfc724_mid_with_acceptable_len12_grpid() {
        let mid = "chat$group.123456789012.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, Some("123456789012"));
    }

    #[test]
    fn test_dc_extract_new_grpid_from_rfc724_mid_with_acceptable_len13_grpid() {
        let mid = "chat$group.1234567890123.morerandom@domain.de";
        let grpid = dc_extract_grpid_from_rfc724_mid(mid);
        assert_eq!(grpid, Some("1234567890123"));
    }

    #[test]
    fn test_emailaddress_parse() {
        assert_eq!("".parse::<EmailAddress>().is_ok(), false);
        assert_eq!(
            "user@domain.tld".parse::<EmailAddress>().unwrap(),
            EmailAddress {
                local: "user".into(),
                domain: "domain.tld".into(),
            }
        );
        assert_eq!("uuu".parse::<EmailAddress>().is_ok(), false);
        assert_eq!("dd.tt".parse::<EmailAddress>().is_ok(), false);
        assert_eq!("tt.dd@uu".parse::<EmailAddress>().is_ok(), false);
        assert_eq!("u@d".parse::<EmailAddress>().is_ok(), false);
        assert_eq!("u@d.".parse::<EmailAddress>().is_ok(), false);
        assert_eq!("u@d.t".parse::<EmailAddress>().is_ok(), false);
        assert_eq!(
            "u@d.tt".parse::<EmailAddress>().unwrap(),
            EmailAddress {
                local: "u".into(),
                domain: "d.tt".into(),
            }
        );
        assert_eq!("u@tt".parse::<EmailAddress>().is_ok(), false);
        assert_eq!("@d.tt".parse::<EmailAddress>().is_ok(), false);
    }

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_dc_truncate(
            buf: String,
            approx_chars in 0..10000usize
        ) {
            let res = dc_truncate(&buf, approx_chars);
            let el_len = 5;
            let l = res.chars().count();
            if approx_chars > 0 {
                assert!(
                    l <= approx_chars + el_len,
                    "buf: '{}' - res: '{}' - len {}, approx {}",
                    &buf, &res, res.len(), approx_chars
                );
            } else {
                assert_eq!(&res, &buf);
            }

            if approx_chars > 0 && buf.chars().count() > approx_chars + el_len {
                let l = res.len();
                assert_eq!(&res[l-5..l], "[...]", "missing ellipsis in {}", &res);
            }
        }
    }

    #[test]
    fn test_file_handling() {
        let t = dummy_context();
        let context = &t.ctx;
        let dc_file_exist = |ctx: &Context, fname: &str| {
            ctx.get_blobdir()
                .join(Path::new(fname).file_name().unwrap())
                .exists()
        };

        assert!(!dc_delete_file(context, "$BLOBDIR/lkqwjelqkwlje"));
        if dc_file_exist(context, "$BLOBDIR/foobar")
            || dc_file_exist(context, "$BLOBDIR/dada")
            || dc_file_exist(context, "$BLOBDIR/foobar.dadada")
            || dc_file_exist(context, "$BLOBDIR/foobar-folder")
        {
            dc_delete_file(context, "$BLOBDIR/foobar");
            dc_delete_file(context, "$BLOBDIR/dada");
            dc_delete_file(context, "$BLOBDIR/foobar.dadada");
            dc_delete_file(context, "$BLOBDIR/foobar-folder");
        }
        assert!(dc_write_file(context, "$BLOBDIR/foobar", b"content").is_ok());
        assert!(dc_file_exist(context, "$BLOBDIR/foobar",));
        assert!(!dc_file_exist(context, "$BLOBDIR/foobarx"));
        assert_eq!(dc_get_filebytes(context, "$BLOBDIR/foobar"), 7);

        let abs_path = context
            .get_blobdir()
            .join("foobar")
            .to_string_lossy()
            .to_string();

        assert!(dc_file_exist(context, &abs_path));

        assert!(dc_copy_file(context, "$BLOBDIR/foobar", "$BLOBDIR/dada",));

        // attempting to copy a second time should fail
        assert!(!dc_copy_file(context, "$BLOBDIR/foobar", "$BLOBDIR/dada",));

        assert_eq!(dc_get_filebytes(context, "$BLOBDIR/dada",), 7);

        let buf = dc_read_file(context, "$BLOBDIR/dada").unwrap();

        assert_eq!(buf.len(), 7);
        assert_eq!(&buf, b"content");

        assert!(dc_delete_file(context, "$BLOBDIR/foobar"));
        assert!(dc_delete_file(context, "$BLOBDIR/dada"));
        assert!(dc_create_folder(context, "$BLOBDIR/foobar-folder").is_ok());
        assert!(dc_file_exist(context, "$BLOBDIR/foobar-folder",));
        assert!(!dc_delete_file(context, "$BLOBDIR/foobar-folder"));

        let fn0 = "$BLOBDIR/data.data";
        assert!(dc_write_file(context, &fn0, b"content").is_ok());

        assert!(dc_delete_file(context, &fn0));
        assert!(!dc_file_exist(context, &fn0));
    }

    #[test]
    fn test_listflags_has() {
        let listflags: u32 = 0x1101;
        assert!(listflags_has(listflags, 0x1));
        assert!(!listflags_has(listflags, 0x10));
        assert!(listflags_has(listflags, 0x100));
        assert!(listflags_has(listflags, 0x1000));
        let listflags: u32 = (DC_GCL_ADD_SELF | DC_GCL_VERIFIED_ONLY).try_into().unwrap();
        assert!(listflags_has(listflags, DC_GCL_VERIFIED_ONLY));
        assert!(listflags_has(listflags, DC_GCL_ADD_SELF));
        let listflags: u32 = DC_GCL_VERIFIED_ONLY.try_into().unwrap();
        assert!(!listflags_has(listflags, DC_GCL_ADD_SELF));
    }

    #[test]
    fn test_create_smeared_timestamp() {
        let t = dummy_context();
        assert_ne!(
            dc_create_smeared_timestamp(&t.ctx),
            dc_create_smeared_timestamp(&t.ctx)
        );
        assert!(
            dc_create_smeared_timestamp(&t.ctx)
                >= SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as i64
        );
    }

    #[test]
    fn test_create_smeared_timestamps() {
        let t = dummy_context();
        let count = MAX_SECONDS_TO_LEND_FROM_FUTURE - 1;
        let start = dc_create_smeared_timestamps(&t.ctx, count as usize);
        let next = dc_smeared_time(&t.ctx);
        assert!((start + count - 1) < next);

        let count = MAX_SECONDS_TO_LEND_FROM_FUTURE + 30;
        let start = dc_create_smeared_timestamps(&t.ctx, count as usize);
        let next = dc_smeared_time(&t.ctx);
        assert!((start + count - 1) < next);
    }
}
