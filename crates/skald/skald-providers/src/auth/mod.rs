//! Provider auth helpers. Secret-bearing structs redact custom `Debug`.

pub mod anthropic;
pub mod google_api_key;
pub mod google_oauth;
pub mod openai;
pub mod vertex;

pub use anthropic::AnthropicAuth;
pub use google_api_key::GoogleApiKeyAuth;
pub use google_oauth::{GoogleOAuth, GoogleOAuthToken};
pub use openai::OpenAiAuth;
pub use vertex::VertexAuth;

fn secret(value: impl Into<String>) -> secrecy::SecretString {
    secrecy::SecretString::new(value.into().into_boxed_str())
}
