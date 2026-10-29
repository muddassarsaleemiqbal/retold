//! Review filters use saved approval evidence; absent approvals remain pending.
use serde_json::Value;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Filter {
    #[default]
    All,
    Pending,
    Stale,
    Takes,
    Failed,
}
impl Filter {
    pub fn next(self) -> Self {
        let filters = [
            Self::All,
            Self::Pending,
            Self::Stale,
            Self::Takes,
            Self::Failed,
        ];
        let index = filters
            .iter()
            .position(|filter| *filter == self)
            .unwrap_or_default();
        filters[(index + 1) % filters.len()]
    }
    pub fn label(self) -> &'static str {
        [
            (Self::All, "all"),
            (Self::Pending, "pending"),
            (Self::Stale, "stale"),
            (Self::Takes, "takes"),
            (Self::Failed, "failed"),
        ]
        .into_iter()
        .find(|(filter, _)| *filter == self)
        .map_or("all", |(_, label)| label)
    }
    pub fn includes(self, asset: &Value, approvals: &[Value]) -> bool {
        let approval = approvals
            .iter()
            .find(|approval| approval["item"] == asset["path"]);
        if self == Self::Failed {
            return matches!(
                asset["status"].as_str(),
                Some("failed" | "error" | "invalid")
            );
        }
        match self {
            Self::All | Self::Failed => true,
            Self::Pending => approval
                .is_none_or(|approval| approval["locked"] != true || approval["stale"] == true),
            Self::Stale => approval.is_some_and(|approval| approval["stale"] == true),
            Self::Takes => asset["is_immutable"] == true,
        }
    }
}
