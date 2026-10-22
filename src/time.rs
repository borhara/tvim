pub(crate) fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

// local hours and minutes, from the C library's idea of the time zone
pub(crate) fn clock() -> (u32, u32) {
    let now = now();
    let mins = (now + utc_offset(now)).rem_euclid(86_400) / 60;
    ((mins / 60) as u32, (mins % 60) as u32)
}

#[cfg(unix)]
// c_long is i32 on 32-bit targets
#[allow(clippy::useless_conversion)]
fn utc_offset(secs: i64) -> i64 {
    #[repr(C)]
    struct Tm {
        fields: [i32; 9],
        gmtoff: std::ffi::c_long,
        zone: *const std::ffi::c_char,
    }
    unsafe extern "C" {
        fn localtime_r(time: *const i64, out: *mut Tm) -> *mut Tm;
    }
    let mut tm = Tm {
        fields: [0; 9],
        gmtoff: 0,
        zone: std::ptr::null(),
    };
    // SAFETY: localtime_r fills one struct tm, whose leading fields have this layout on macOS and
    // Linux; on failure it returns null and gmtoff stays 0
    unsafe { localtime_r(&secs, &mut tm) };
    i64::from(tm.gmtoff)
}

#[cfg(not(unix))]
fn utc_offset(_: i64) -> i64 {
    0
}
