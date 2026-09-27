pub(crate) struct Task<T> {
    #[cfg(not(target_arch = "wasm32"))]
    handle: std::thread::JoinHandle<T>,
    #[cfg(target_arch = "wasm32")]
    answer: std::thread::Result<T>,
}

pub(crate) fn spawn<T, F>(job: F) -> Task<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    #[cfg(not(target_arch = "wasm32"))]
    {
        Task {
            handle: std::thread::spawn(job),
        }
    }
    #[cfg(target_arch = "wasm32")]
    {
        Task { answer: Ok(job()) }
    }
}

impl<T> Task<T> {
    pub(crate) fn is_finished(&self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.handle.is_finished()
        }
        #[cfg(target_arch = "wasm32")]
        {
            true
        }
    }

    pub(crate) fn join(self) -> std::thread::Result<T> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.handle.join()
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.answer
        }
    }
}
