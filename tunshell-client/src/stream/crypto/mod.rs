cfg_if::cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod web_sys;
        pub(super) use self::web_sys::*;
    } else if #[cfg(openssl)] {
        mod openssl;
        pub use self::openssl::*;
    } else {
        mod ring;
        pub use self::ring::*;
    }
}
