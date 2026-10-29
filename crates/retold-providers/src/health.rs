//! Shared health admission, independent of billing authority.
use crate::ProviderError;

/// Implementations must persist failures before returning and fail closed on storage errors.
pub trait Health: std::fmt::Debug + Send + Sync {
    fn check(&self, provider: &str) -> Result<(), ProviderError>;
    fn failed(&self, provider: &str, error: &ProviderError) -> Result<(), ProviderError>;

    fn succeeded(&self, _provider: &str) -> Result<(), ProviderError> {
        Ok(())
    }
}
