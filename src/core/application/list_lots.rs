use crate::core::domain::{DomainError, IrrigationSystem, Texture, YieldTarget};
use crate::core::ports::{FieldContextRepository, ListLotsPort, YieldTargetRepository};

/// A lot as a picker needs it. `curated_targets` is empty for a lot
/// registered without a planning row — a normal state, and why the list
/// comes from the field contexts rather than from `yield_targets.csv`.
#[derive(Debug, Clone)]
pub struct LotSummary {
    pub field_id: String,
    pub texture: Texture,
    pub irrigation_system: IrrigationSystem,
    pub curated_targets: Vec<(String, YieldTarget)>,
    /// Carried so a front-end can warm a climatology before a plan is
    /// asked for. `None` for a lot with no surveyed position.
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
}

impl LotSummary {
    pub fn target_for(&self, crop_id: &str) -> Option<&YieldTarget> {
        self.curated_targets
            .iter()
            .find(|(crop, _)| crop == crop_id)
            .map(|(_, target)| target)
    }

    /// The crop shown next to the lot when the user hasn't picked one.
    pub fn default_crop(&self) -> Option<&str> {
        self.curated_targets.first().map(|(crop, _)| crop.as_str())
    }
}

pub struct ListLots {
    field_context: Box<dyn FieldContextRepository>,
    yield_targets: Box<dyn YieldTargetRepository>,
}

impl ListLots {
    pub fn new(field_context: Box<dyn FieldContextRepository>, yield_targets: Box<dyn YieldTargetRepository>) -> Self {
        Self { field_context, yield_targets }
    }
}

impl ListLotsPort for ListLots {
    fn list_lots(&self) -> Result<Vec<LotSummary>, DomainError> {
        let targets = self.yield_targets.list_targets()?;
        Ok(self
            .field_context
            .list_contexts()?
            .into_iter()
            .map(|context| LotSummary {
                curated_targets: targets
                    .iter()
                    .filter(|target| target.field_id == context.field_id)
                    .map(|target| (target.crop_id.clone(), target.target.clone()))
                    .collect(),
                field_id: context.field_id,
                texture: context.texture,
                irrigation_system: context.irrigation_system,
                latitude: context.latitude,
                longitude: context.longitude,
            })
            .collect())
    }
}
