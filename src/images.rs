//! Loads icons over HTTP(S) with Paprika's own client (proxy aware, cached).

use eframe::egui::load::{Bytes, BytesLoadResult, BytesLoader, BytesPoll, LoadError};
use eframe::egui::{self, mutex::Mutex};
use std::collections::HashMap;
use std::io::Read;
use std::sync::Arc;

type Entry = Option<Result<Arc<[u8]>, String>>;

#[derive(Default)]
pub struct HttpLoader {
    cache: Arc<Mutex<HashMap<String, Entry>>>,
}

impl BytesLoader for HttpLoader {
    fn id(&self) -> &str {
        "paprika::HttpLoader"
    }

    fn load(&self, ctx: &egui::Context, uri: &str) -> BytesLoadResult {
        if !(uri.starts_with("https://") || uri.starts_with("http://")) {
            return Err(LoadError::NotSupported);
        }
        let mut cache = self.cache.lock();
        match cache.get(uri) {
            Some(Some(Ok(bytes))) => Ok(BytesPoll::Ready {
                size: None,
                bytes: Bytes::Shared(bytes.clone()),
                mime: None,
            }),
            Some(Some(Err(e))) => Err(LoadError::Loading(e.clone())),
            Some(None) => Ok(BytesPoll::Pending { size: None }),
            None => {
                cache.insert(uri.to_string(), None);
                drop(cache);
                let (uri, cache, ctx) = (uri.to_string(), self.cache.clone(), ctx.clone());
                std::thread::spawn(move || {
                    let res = crate::net::agent()
                        .get(&uri)
                        .call()
                        .map_err(|e| e.to_string())
                        .and_then(|r| {
                            let mut buf = Vec::new();
                            r.into_reader()
                                .take(8 * 1024 * 1024)
                                .read_to_end(&mut buf)
                                .map_err(|e| e.to_string())?;
                            Ok(Arc::<[u8]>::from(buf))
                        });
                    cache.lock().insert(uri, Some(res));
                    ctx.request_repaint();
                });
                Ok(BytesPoll::Pending { size: None })
            }
        }
    }

    fn forget(&self, uri: &str) {
        self.cache.lock().remove(uri);
    }

    fn forget_all(&self) {
        self.cache.lock().clear();
    }

    fn byte_size(&self) -> usize {
        self.cache
            .lock()
            .values()
            .map(|v| match v {
                Some(Ok(b)) => b.len(),
                _ => 0,
            })
            .sum()
    }
}
