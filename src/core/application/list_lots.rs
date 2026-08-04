use crate::core::domain::{DomainError, IrrigationSystem, Texture, YieldTarget};
use crate::core::ports::{FieldContextRepository, ListLotsPort, YieldTargetRepository};

/// A lot as a picker needs it. `curated_targets` is empty for a lot
/// registered without a planning row — a normal state, and why the list
/// comes from the field contexts rather than from `yield_targets.csv`.
#[derive(Debug, Clone)]
pub struct LotSummary {
    /// The lot's id.
    pub field_id: String,
    /// Its texture, shown so a picker can tell two lots apart without
    /// opening either.
    pub texture: Texture,
    /// How it is watered.
    pub irrigation_system: IrrigationSystem,
    /// Every `(crop_id, goal)` curated for this lot, in file order. Empty
    /// for a lot registered with nothing planned on it.
    pub curated_targets: Vec<(String, YieldTarget)>,
    /// Carried so a front-end can warm a climatology before a plan is
    /// asked for. `None` for a lot with no surveyed position.
    pub latitude: Option<f64>,
    /// Carried alongside `latitude`; either one missing means no
    /// climatology can be warmed for this lot.
    pub longitude: Option<f64>,
}

impl LotSummary {
    /// The curated goal for one crop on this lot.
    ///
    /// # Arguments
    /// * `crop_id` — the crop to look up.
    ///
    /// # Returns
    /// `None` when nothing is planned for that crop here, which is what
    /// makes a front-end ask for a goal rather than assume one.
    #[must_use]
    pub fn target_for(&self, crop_id: &str) -> Option<&YieldTarget> {
        self.curated_targets
            .iter()
            .find(|(crop, _)| crop == crop_id)
            .map(|(_, target)| target)
    }

    /// The crop shown next to the lot when the user hasn't picked one.
    #[must_use]
    pub fn default_crop(&self) -> Option<&str> {
        self.curated_targets.first().map(|(crop, _)| crop.as_str())
    }
}

/// Lists the curated lots with enough of each to choose one.
pub struct ListLots {
    field_context: Box<dyn FieldContextRepository>,
    yield_targets: Box<dyn YieldTargetRepository>,
}

impl ListLots {
    /// # Arguments
    /// * `field_context` — the lots themselves.
    /// * `yield_targets` — their planning rows, joined onto each lot.
    ///
    /// # Returns
    /// The use case, ready to list.
    #[must_use]
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
