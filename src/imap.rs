use crate::types::*;
use libc;

extern "C" {
    pub fn mailimap_date_time_new(
        dt_day: libc::c_int,
        dt_month: libc::c_int,
        dt_year: libc::c_int,
        dt_hour: libc::c_int,
        dt_min: libc::c_int,
        dt_sec: libc::c_int,
        dt_zone: libc::c_int,
    ) -> *mut mailimap_date_time;
    pub fn mailimap_xlist(
        session: *mut mailimap,
        mb: *const libc::c_char,
        list_mb: *const libc::c_char,
        result: *mut *mut clist,
    ) -> libc::c_int;
    pub fn mailimap_create(session: *mut mailimap, mb: *const libc::c_char) -> libc::c_int;
    pub fn mailimap_list(
        session: *mut mailimap,
        mb: *const libc::c_char,
        list_mb: *const libc::c_char,
        result: *mut *mut clist,
    ) -> libc::c_int;
    pub fn mailimap_list_result_free(list: *mut clist);
    pub fn mailimap_subscribe(session: *mut mailimap, mb: *const libc::c_char) -> libc::c_int;
    pub fn mailstream_close(s: *mut mailstream) -> libc::c_int;
    pub fn mailstream_wait_idle(s: *mut mailstream, max_idle_delay: libc::c_int) -> libc::c_int;
    pub fn mailstream_setup_idle(s: *mut mailstream) -> libc::c_int;
    pub fn mailstream_unsetup_idle(s: *mut mailstream);
    pub fn mailstream_interrupt_idle(s: *mut mailstream);
    pub fn mailimap_section_new(sec_spec: *mut mailimap_section_spec) -> *mut mailimap_section;
    pub fn mailimap_set_free(set: *mut mailimap_set);
    pub fn mailimap_fetch_type_free(fetch_type: *mut mailimap_fetch_type);
    pub fn mailimap_store_att_flags_free(store_att_flags: *mut mailimap_store_att_flags);
    pub fn mailimap_set_new_interval(first: uint32_t, last: uint32_t) -> *mut mailimap_set;
    pub fn mailimap_set_new_single(indx: uint32_t) -> *mut mailimap_set;
    pub fn mailimap_fetch_att_new_envelope() -> *mut mailimap_fetch_att;
    pub fn mailimap_fetch_att_new_flags() -> *mut mailimap_fetch_att;
    pub fn mailimap_fetch_att_new_uid() -> *mut mailimap_fetch_att;
    pub fn mailimap_fetch_att_new_body_peek_section(
        section: *mut mailimap_section,
    ) -> *mut mailimap_fetch_att;
    pub fn mailimap_fetch_type_new_fetch_att_list_empty() -> *mut mailimap_fetch_type;
    pub fn mailimap_fetch_type_new_fetch_att_list_add(
        fetch_type: *mut mailimap_fetch_type,
        fetch_att: *mut mailimap_fetch_att,
    ) -> libc::c_int;
    pub fn mailimap_store_att_flags_new_add_flags(
        flags: *mut mailimap_flag_list,
    ) -> *mut mailimap_store_att_flags;
    pub fn mailimap_flag_list_new_empty() -> *mut mailimap_flag_list;
    pub fn mailimap_flag_list_add(
        flag_list: *mut mailimap_flag_list,
        f: *mut mailimap_flag,
    ) -> libc::c_int;
    pub fn mailimap_flag_new_deleted() -> *mut mailimap_flag;
    pub fn mailimap_flag_new_seen() -> *mut mailimap_flag;
    pub fn mailimap_flag_new_flag_keyword(flag_keyword: *mut libc::c_char) -> *mut mailimap_flag;
    pub fn mailimap_socket_connect(
        f: *mut mailimap,
        server: *const libc::c_char,
        port: uint16_t,
    ) -> libc::c_int;
    pub fn mailimap_socket_starttls(f: *mut mailimap) -> libc::c_int;
    pub fn mailimap_ssl_connect(
        f: *mut mailimap,
        server: *const libc::c_char,
        port: uint16_t,
    ) -> libc::c_int;
    pub fn mailimap_uidplus_uid_copy(
        session: *mut mailimap,
        set: *mut mailimap_set,
        mb: *const libc::c_char,
        uidvalidity_result: *mut uint32_t,
        source_result: *mut *mut mailimap_set,
        dest_result: *mut *mut mailimap_set,
    ) -> libc::c_int;
    pub fn mailimap_uidplus_uid_move(
        session: *mut mailimap,
        set: *mut mailimap_set,
        mb: *const libc::c_char,
        uidvalidity_result: *mut uint32_t,
        source_result: *mut *mut mailimap_set,
        dest_result: *mut *mut mailimap_set,
    ) -> libc::c_int;
    pub fn mailimap_idle(session: *mut mailimap) -> libc::c_int;
    pub fn mailimap_idle_done(session: *mut mailimap) -> libc::c_int;
    pub fn mailimap_has_idle(session: *mut mailimap) -> libc::c_int;
    pub fn mailimap_has_xlist(session: *mut mailimap) -> libc::c_int;
    pub fn mailimap_oauth2_authenticate(
        session: *mut mailimap,
        auth_user: *const libc::c_char,
        access_token: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailimap_close(session: *mut mailimap) -> libc::c_int;
    pub fn mailimap_fetch(
        session: *mut mailimap,
        set: *mut mailimap_set,
        fetch_type: *mut mailimap_fetch_type,
        result: *mut *mut clist,
    ) -> libc::c_int;
    pub fn mailimap_uid_fetch(
        session: *mut mailimap,
        set: *mut mailimap_set,
        fetch_type: *mut mailimap_fetch_type,
        result: *mut *mut clist,
    ) -> libc::c_int;
    pub fn mailimap_fetch_list_free(fetch_list: *mut clist);
    pub fn mailimap_login(
        session: *mut mailimap,
        userid: *const libc::c_char,
        password: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailimap_select(session: *mut mailimap, mb: *const libc::c_char) -> libc::c_int;
    pub fn mailimap_uid_store(
        session: *mut mailimap,
        set: *mut mailimap_set,
        store_att_flags: *mut mailimap_store_att_flags,
    ) -> libc::c_int;
    pub fn mailimap_new(
        imap_progr_rate: size_t,
        imap_progr_fun: Option<unsafe extern "C" fn(_: size_t, _: size_t) -> ()>,
    ) -> *mut mailimap;
    pub fn mailimap_free(session: *mut mailimap);
    pub fn mailimap_set_timeout(session: *mut mailimap, timeout: time_t);
}
