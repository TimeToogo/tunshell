use cfg_if::cfg_if;
use std::time::Duration;

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        use std::sync::{Arc, Mutex};
        use std::task::Poll;
        use wasm_bindgen::prelude::*;

        #[wasm_bindgen]
        extern "C" {
            fn setTimeout(closure: &Closure<dyn FnMut()>, millis: u32) -> f64;
        }


        pub async fn delay_for(duration: Duration) {
            let woken = Arc::new(Mutex::new(false));

            futures::future::poll_fn(move |cx| {
                if *woken.lock().unwrap() {
                    return Poll::Ready(());
                }

                let callback = {
                    let waker = cx.waker().clone();
                    let woken = Arc::clone(&woken);

                    Closure::wrap(Box::new(move || {
                        let mut woken = woken.lock().unwrap();
                        *woken = true;
                        waker.clone().wake();
                    }) as Box<dyn FnMut()>)
                };

                setTimeout(&callback, duration.as_millis() as u32);
                callback.forget();
                Poll::Pending
            }).await
        }
    } else {
        use tokio::time;

        pub async fn delay_for(duration: Duration) {
            time::delay_for(duration).await
        }
    }
}
