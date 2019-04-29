use libc;

use crate::types::*;

extern "C" {
    pub fn mailmime_base64_body_parse(
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        result: *mut *mut libc::c_char,
        result_len: *mut size_t,
    ) -> libc::c_int;
    pub fn mailimf_address_new(
        ad_type: libc::c_int,
        ad_mailbox: *mut mailimf_mailbox,
        ad_group: *mut mailimf_group,
    ) -> *mut mailimf_address;
    pub fn mailimf_mailbox_new(
        mb_display_name: *mut libc::c_char,
        mb_addr_spec: *mut libc::c_char,
    ) -> *mut mailimf_mailbox;
    pub fn mailimf_field_new(
        fld_type: libc::c_int,
        fld_return_path: *mut mailimf_return,
        fld_resent_date: *mut mailimf_orig_date,
        fld_resent_from: *mut mailimf_from,
        fld_resent_sender: *mut mailimf_sender,
        fld_resent_to: *mut mailimf_to,
        fld_resent_cc: *mut mailimf_cc,
        fld_resent_bcc: *mut mailimf_bcc,
        fld_resent_msg_id: *mut mailimf_message_id,
        fld_orig_date: *mut mailimf_orig_date,
        fld_from: *mut mailimf_from,
        fld_sender: *mut mailimf_sender,
        fld_reply_to: *mut mailimf_reply_to,
        fld_to: *mut mailimf_to,
        fld_cc: *mut mailimf_cc,
        fld_bcc: *mut mailimf_bcc,
        fld_message_id: *mut mailimf_message_id,
        fld_in_reply_to: *mut mailimf_in_reply_to,
        fld_references: *mut mailimf_references,
        fld_subject: *mut mailimf_subject,
        fld_comments: *mut mailimf_comments,
        fld_keywords: *mut mailimf_keywords,
        fld_optional_field: *mut mailimf_optional_field,
    ) -> *mut mailimf_field;
    pub fn mailimf_subject_new(sbj_value: *mut libc::c_char) -> *mut mailimf_subject;
    pub fn mailimf_mailbox_list_new_empty() -> *mut mailimf_mailbox_list;
    pub fn mailimf_mailbox_list_add(
        mailbox_list: *mut mailimf_mailbox_list,
        mb: *mut mailimf_mailbox,
    ) -> libc::c_int;
    pub fn mailimf_address_list_new_empty() -> *mut mailimf_address_list;
    pub fn mailimf_address_list_add(
        address_list: *mut mailimf_address_list,
        addr: *mut mailimf_address,
    ) -> libc::c_int;
    pub fn mailimf_fields_add(
        fields: *mut mailimf_fields,
        field: *mut mailimf_field,
    ) -> libc::c_int;
    pub fn mailimf_fields_new_with_data_all(
        date: *mut mailimf_date_time,
        from: *mut mailimf_mailbox_list,
        sender: *mut mailimf_mailbox,
        reply_to: *mut mailimf_address_list,
        to: *mut mailimf_address_list,
        cc: *mut mailimf_address_list,
        bcc: *mut mailimf_address_list,
        message_id: *mut libc::c_char,
        in_reply_to: *mut clist,
        references: *mut clist,
        subject: *mut libc::c_char,
    ) -> *mut mailimf_fields;
    pub fn mailimf_get_date(time_0: time_t) -> *mut mailimf_date_time;
    pub fn mailimf_field_new_custom(
        name: *mut libc::c_char,
        value: *mut libc::c_char,
    ) -> *mut mailimf_field;
    pub fn mailmime_parameter_new(
        pa_name: *mut libc::c_char,
        pa_value: *mut libc::c_char,
    ) -> *mut mailmime_parameter;
    pub fn mailmime_free(mime: *mut mailmime);
    pub fn mailmime_disposition_parm_new(
        pa_type: libc::c_int,
        pa_filename: *mut libc::c_char,
        pa_creation_date: *mut libc::c_char,
        pa_modification_date: *mut libc::c_char,
        pa_read_date: *mut libc::c_char,
        pa_size: size_t,
        pa_parameter: *mut mailmime_parameter,
    ) -> *mut mailmime_disposition_parm;
    pub fn mailmime_new_message_data(msg_mime: *mut mailmime) -> *mut mailmime;
    pub fn mailmime_new_empty(
        content: *mut mailmime_content,
        mime_fields: *mut mailmime_fields,
    ) -> *mut mailmime;
    pub fn mailmime_set_body_file(
        build_info: *mut mailmime,
        filename: *mut libc::c_char,
    ) -> libc::c_int;
    pub fn mailmime_set_body_text(
        build_info: *mut mailmime,
        data_str: *mut libc::c_char,
        length: size_t,
    ) -> libc::c_int;
    pub fn mailmime_add_part(build_info: *mut mailmime, part: *mut mailmime) -> libc::c_int;
    pub fn mailmime_set_imf_fields(build_info: *mut mailmime, fields: *mut mailimf_fields);
    pub fn mailmime_smart_add_part(mime: *mut mailmime, mime_sub: *mut mailmime) -> libc::c_int;
    pub fn mailmime_content_new_with_str(str: *const libc::c_char) -> *mut mailmime_content;
    pub fn mailmime_fields_new_encoding(type_0: libc::c_int) -> *mut mailmime_fields;
    pub fn mailmime_multiple_new(type_0: *const libc::c_char) -> *mut mailmime;
    pub fn mailmime_fields_new_filename(
        dsp_type: libc::c_int,
        filename: *mut libc::c_char,
        encoding_type: libc::c_int,
    ) -> *mut mailmime_fields;
    pub fn mailmime_param_new_with_data(
        name: *mut libc::c_char,
        value: *mut libc::c_char,
    ) -> *mut mailmime_parameter;
    pub fn mailmime_write_mem(
        f: *mut MMAPString,
        col: *mut libc::c_int,
        build_info: *mut mailmime,
    ) -> libc::c_int;
    pub fn mailimf_fields_free(fields: *mut mailimf_fields);
    pub fn mailimf_fields_new_empty() -> *mut mailimf_fields;
    pub fn mailimf_envelope_and_optional_fields_parse(
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        result: *mut *mut mailimf_fields,
    ) -> libc::c_int;
    pub fn mailmime_content_free(content: *mut mailmime_content);
    pub fn mailmime_mechanism_new(
        enc_type: libc::c_int,
        enc_token: *mut libc::c_char,
    ) -> *mut mailmime_mechanism;
    pub fn mailmime_mechanism_free(mechanism: *mut mailmime_mechanism);
    pub fn mailmime_fields_free(fields: *mut mailmime_fields);
    pub fn mailmime_new(
        mm_type: libc::c_int,
        mm_mime_start: *const libc::c_char,
        mm_length: size_t,
        mm_mime_fields: *mut mailmime_fields,
        mm_content_type: *mut mailmime_content,
        mm_body: *mut mailmime_data,
        mm_preamble: *mut mailmime_data,
        mm_epilogue: *mut mailmime_data,
        mm_mp_list: *mut clist,
        mm_fields: *mut mailimf_fields,
        mm_msg_mime: *mut mailmime,
    ) -> *mut mailmime;
    pub fn mailmime_fields_new_empty() -> *mut mailmime_fields;
    pub fn mailmime_fields_new_with_data(
        encoding: *mut mailmime_mechanism,
        id: *mut libc::c_char,
        description: *mut libc::c_char,
        disposition: *mut mailmime_disposition,
        language: *mut mailmime_language,
    ) -> *mut mailmime_fields;
    pub fn mailmime_get_content_message() -> *mut mailmime_content;
    pub fn mailmime_parse(
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        result: *mut *mut mailmime,
    ) -> libc::c_int;
    pub fn mailmime_part_parse(
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        encoding: libc::c_int,
        result: *mut *mut libc::c_char,
        result_len: *mut size_t,
    ) -> libc::c_int;
    pub fn mailmime_substitute(old_mime: *mut mailmime, new_mime: *mut mailmime) -> libc::c_int;
    pub fn mailprivacy_prepare_mime(mime: *mut mailmime);
    pub fn mailmime_encoded_phrase_parse(
        default_fromcode: *const libc::c_char,
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        tocode: *const libc::c_char,
        result: *mut *mut libc::c_char,
    ) -> libc::c_int;

    pub fn mailimf_msg_id_parse(
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        result: *mut *mut libc::c_char,
    ) -> libc::c_int;
    pub fn mailimf_mailbox_list_free(mb_list: *mut mailimf_mailbox_list);
    pub fn mailimf_mailbox_list_parse(
        message: *const libc::c_char,
        length: size_t,
        indx: *mut size_t,
        result: *mut *mut mailimf_mailbox_list,
    ) -> libc::c_int;
    pub fn mailmime_content_charset_get(content: *mut mailmime_content) -> *mut libc::c_char;

    pub fn carray_new(initsize: libc::c_uint) -> *mut carray;
    pub fn carray_add(
        array: *mut carray,
        data: *mut libc::c_void,
        indx: *mut libc::c_uint,
    ) -> libc::c_int;
    pub fn carray_set_size(array: *mut carray, new_size: libc::c_uint);
    pub fn carray_free(array: *mut carray);
    pub fn carray_delete_slow(array: *mut carray, indx: libc::c_uint) -> libc::c_int;

    pub fn mmap_string_unref(str: *mut libc::c_char) -> libc::c_int;
    pub fn mmap_string_new(init: *const libc::c_char) -> *mut MMAPString;
    pub fn mmap_string_free(string: *mut MMAPString);
    pub fn mmap_string_append(string: *mut MMAPString, val: *const libc::c_char)
        -> *mut MMAPString;
    pub fn mmap_string_append_len(
        string: *mut MMAPString,
        val: *const libc::c_char,
        len: size_t,
    ) -> *mut MMAPString;
    pub fn mmap_string_append_c(string: *mut MMAPString, c: libc::c_char) -> *mut MMAPString;

    pub fn clist_free(_: *mut clist);
    pub fn clist_insert_after(
        _: *mut clist,
        _: *mut clistiter,
        _: *mut libc::c_void,
    ) -> libc::c_int;
    pub fn clist_new() -> *mut clist;
    pub fn clist_delete(_: *mut clist, _: *mut clistiter) -> *mut clistiter;

    // --charconv

    pub fn charconv(
        tocode: *const libc::c_char,
        fromcode: *const libc::c_char,
        str: *const libc::c_char,
        length: size_t,
        result: *mut *mut libc::c_char,
    ) -> libc::c_int;
    pub fn charconv_buffer(
        tocode: *const libc::c_char,
        fromcode: *const libc::c_char,
        str: *const libc::c_char,
        length: size_t,
        result: *mut *mut libc::c_char,
        result_len: *mut size_t,
    ) -> libc::c_int;
    pub fn charconv_buffer_free(str: *mut libc::c_char);
}
