pub mod handler;

pub trait Service {
    fn serve(&self) -> Result<(), AppError>;
}

pub enum AppError {
    NotFound,
    Internal(String),
}
