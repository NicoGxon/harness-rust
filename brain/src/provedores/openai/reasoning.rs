use std::fmt;

/// Esfuerzo de razonamiento admitido por Rig 0.40 para Responses API.
///
/// La ausencia del valor (`None` en la configuración) significa que el
/// proveedor decide el nivel por defecto para el modelo seleccionado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    None,
    Minimal,
    Low,
    Medium,
    High,
    Xhigh,
}

impl ReasoningEffort {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Minimal,
        Self::Low,
        Self::Medium,
        Self::High,
        Self::Xhigh,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::Xhigh => "xhigh",
        }
    }
}

impl fmt::Display for ReasoningEffort {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ReasoningEffort;

    #[test]
    fn serializes_effort_as_responses_api_value() {
        assert_eq!(ReasoningEffort::High.as_str(), "high");
        assert_eq!(
            serde_json::to_string(&ReasoningEffort::Xhigh).unwrap(),
            "\"xhigh\""
        );
    }
}
