//! The crop catalog as a front-end needs it to offer a choice.
//!
//! A read-only pass over one reference table — thin on purpose, so that
//! "which crops exist" is answered through a port like everything else
//! rather than by a front-end opening the CSV itself.

use crate::core::domain::{Crop, DomainError};
use crate::core::ports::{CropCatalogRepository, ListCropsPort};

/// Lists the crops the catalog knows.
pub struct ListSupportedCrops {
    crop_catalog: Box<dyn CropCatalogRepository>,
}

impl ListSupportedCrops {
    /// # Arguments
    /// * `crop_catalog` — where the crops are read from.
    ///
    /// # Returns
    /// The use case, ready to answer.
    #[must_use]
    pub fn new(crop_catalog: Box<dyn CropCatalogRepository>) -> Self {
        Self { crop_catalog }
    }
}

impl ListCropsPort for ListSupportedCrops {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError> {
        self.crop_catalog.list_crops()
    }
}
