//! Haiku 호환 셔밍. Rust std는 Haiku에서 arc4random_buf를 호출하지만
//! R1beta4 libroot에는 없다(R1beta5에서 추가). /dev/urandom으로 대체한다.
//! 실행 파일 안의 정의가 우선하므로 최신 Haiku에서도 그대로 동작한다.

use libc::{O_CLOEXEC, O_RDONLY, c_void, size_t};

#[unsafe(no_mangle)]
pub unsafe extern "C" fn arc4random_buf(buf: *mut c_void, len: size_t) {
    unsafe {
        let fd = libc::open(c"/dev/urandom".as_ptr(), O_RDONLY | O_CLOEXEC);
        if fd < 0 {
            libc::abort();
        }
        let mut done = 0usize;
        while done < len {
            let n = libc::read(fd, buf.add(done), len - done);
            if n < 0 && *libc::_errnop() == libc::EINTR {
                continue;
            }
            if n <= 0 {
                libc::abort();
            }
            done += n as usize;
        }
        libc::close(fd);
    }
}
