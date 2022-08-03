use crate::digesters::ImmutableFileListingError;
use crate::entities::ImmutableFileNumber;
use async_trait::async_trait;
use std::io;
use thiserror::Error;

/// A digester than can compute the digest used for mithril signatures
///
/// If you want to mock it using mockall:
/// ```
/// mod test {
///     use async_trait::async_trait;
///     use mithril_common::digesters::{ImmutableDigester, ImmutableDigesterError};
///     use mithril_common::entities::ImmutableFileNumber;
///     use mockall::mock;
///
///     mock! {
///         pub ImmutableDigesterImpl { }
///
///         #[async_trait]
///         impl ImmutableDigester for ImmutableDigesterImpl {
///             async fn compute_digest(
///               &self,
///               up_to_file_number: ImmutableFileNumber,
///             ) -> Result<String, ImmutableDigesterError>;
///         }
///     }
///
///     #[test]
///     fn test_mock() {
///         let mut mock = MockDigesterImpl::new();
///         mock.expect_compute_digest().return_once(|_| {
///             Err(ImmutableDigesterError::NotEnoughImmutable {
///                 expected_number: 3,
///                 found_number: None,
///             })
///         });
///     }
/// }
/// ```
#[async_trait]
pub trait ImmutableDigester: Sync + Send {
    /// Compute the digest
    async fn compute_digest(
        &self,
        up_to_file_number: ImmutableFileNumber,
    ) -> Result<String, ImmutableDigesterError>;
}

/// [ImmutableDigester] related Errors.
#[derive(Error, Debug)]
pub enum ImmutableDigesterError {
    /// Error raised when the files listing failed.
    #[error("Immutable files listing failed: {0}")]
    ListImmutablesError(#[from] ImmutableFileListingError),

    /// Error raised when there's less than the required number of completed immutables in
    /// the cardano database or even no immutable at all.
    #[error("At least two immutables chunk should exists")]
    NotEnoughImmutable {
        /// Expected last [ImmutableFileNumber].
        expected_number: ImmutableFileNumber,
        /// Last [ImmutableFileNumber] found when listing [ImmutableFiles][crate::digesters::ImmutableFile].
        found_number: Option<ImmutableFileNumber>,
    },

    /// Error raised when the digest computation failed.
    #[error("Digest computation failed: {0}")]
    DigestComputationError(#[from] io::Error),
}

/// A [ImmutableDigester] returning configurable result for testing purpose.
pub struct DumbImmutableDigester {
    digest: String,
    is_success: bool,
}

impl DumbImmutableDigester {
    /// DumbDigester factory
    pub fn new(digest: &str, is_success: bool) -> Self {
        let digest = String::from(digest);

        Self { digest, is_success }
    }
}

impl Default for DumbImmutableDigester {
    fn default() -> Self {
        Self::new("1234", true)
    }
}

#[async_trait]
impl ImmutableDigester for DumbImmutableDigester {
    async fn compute_digest(
        &self,
        up_to_file_number: ImmutableFileNumber,
    ) -> Result<String, ImmutableDigesterError> {
        if self.is_success {
            Ok(self.digest.clone())
        } else {
            Err(ImmutableDigesterError::NotEnoughImmutable {
                expected_number: up_to_file_number,
                found_number: None,
            })
        }
    }
}
