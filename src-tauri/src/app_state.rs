use std::sync::Mutex;
use crate::{error::AppError, storage::Database};

pub struct AppState { pub db: Mutex<Database> }

impl AppState {
    pub fn new() -> Result<Self, AppError> { Ok(Self { db: Mutex::new(Database::open_default()?) }) }
}
