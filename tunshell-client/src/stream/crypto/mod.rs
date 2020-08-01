cfg_if::cfg_if! {
    if #[cfg(not(target_arch = "wasm32"))] {
        mod ring;
        pub use self::ring::*;
    } else {
        mod web_sys;
        pub(super) use self::web_sys::*;
    }
}
