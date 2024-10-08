use tokio::sync::Mutex;

#[derive(Debug)]
pub struct AuthService;

impl AuthService {
    pub async fn authorize(&self) -> bool {
        true
    }
}

pub type SharedAuthService = Mutex<AuthService>;

pub fn new_auth_service() -> SharedAuthService {
    Mutex::new(AuthService)
}