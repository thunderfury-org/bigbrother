use std::sync::Arc;

use crate::client::pan123;

#[derive(Clone)]
pub struct AppState {
    pub config: super::config::Manager,
    pub pan123: Arc<pan123::Client>,
}
