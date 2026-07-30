use crate::core::domain::{Crop, DomainError};
use crate::core::ports::{CropCatalogRepository, ListCropsPort};

pub struct ListSupportedCrops {
    crop_catalog: Box<dyn CropCatalogRepository>,
}

impl ListSupportedCrops {
    pub fn new(crop_catalog: Box<dyn CropCatalogRepository>) -> Self {
        Self { crop_catalog }
    }
}

impl ListCropsPort for ListSupportedCrops {
    fn list_crops(&self) -> Result<Vec<Crop>, DomainError> {
        self.crop_catalog.list_crops()
    }
}
