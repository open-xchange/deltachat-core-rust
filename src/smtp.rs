use crate::types::*;
use libc;

extern "C" {
    pub fn gethostname(_: *mut libc::c_char, _: size_t) -> libc::c_int;

    pub fn mailsmtp_socket_connect(
        session: *mut mailsmtp,
        server: *const libc::c_char,
        port: uint16_t,
    ) -> libc::c_int;
    pub fn mailsmtp_socket_starttls(session: *mut mailsmtp) -> libc::c_int;
    pub fn mailsmtp_ssl_connect(
        session: *mut mailsmtp,
        server: *const libc::c_char,
        port: uint16_t,
    ) -> libc::c_int;
    pub fn mailsmtp_oauth2_authenticate(
        session: *mut mailsmtp,
        auth_user: *const libc::c_char,
        access_token: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailsmtp_new(
        progr_rate: size_t,
        progr_fun: Option<unsafe extern "C" fn(_: size_t, _: size_t) -> ()>,
    ) -> *mut mailsmtp;
    pub fn mailsmtp_free(session: *mut mailsmtp);
    pub fn mailsmtp_set_timeout(session: *mut mailsmtp, timeout: time_t);
    pub fn mailsmtp_auth(
        session: *mut mailsmtp,
        user: *const libc::c_char,
        pass: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailsmtp_helo(session: *mut mailsmtp) -> libc::c_int;
    pub fn mailsmtp_mail(session: *mut mailsmtp, from: *const libc::c_char) -> libc::c_int;
    pub fn mailsmtp_rcpt(session: *mut mailsmtp, to: *const libc::c_char) -> libc::c_int;
    pub fn mailsmtp_data(session: *mut mailsmtp) -> libc::c_int;
    pub fn mailsmtp_data_message(
        session: *mut mailsmtp,
        message: *const libc::c_char,
        size: size_t,
    ) -> libc::c_int;
    pub fn mailesmtp_ehlo(session: *mut mailsmtp) -> libc::c_int;
    pub fn mailesmtp_mail(
        session: *mut mailsmtp,
        from: *const libc::c_char,
        return_full: libc::c_int,
        envid: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailesmtp_rcpt(
        session: *mut mailsmtp,
        to: *const libc::c_char,
        notify: libc::c_int,
        orcpt: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailsmtp_strerror(errnum: libc::c_int) -> *const libc::c_char;
    pub fn mailesmtp_auth_sasl(
        session: *mut mailsmtp,
        auth_type: *const libc::c_char,
        server_fqdn: *const libc::c_char,
        local_ip_port: *const libc::c_char,
        remote_ip_port: *const libc::c_char,
        login: *const libc::c_char,
        auth_name: *const libc::c_char,
        password: *const libc::c_char,
        realm: *const libc::c_char,
    ) -> libc::c_int;
    pub fn mailsmtp_set_progress_callback(
        session: *mut mailsmtp,
        progr_fun: Option<unsafe extern "C" fn(_: size_t, _: size_t, _: *mut libc::c_void) -> ()>,
        context: *mut libc::c_void,
    );
}
