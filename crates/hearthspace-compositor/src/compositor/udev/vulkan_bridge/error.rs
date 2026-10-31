use ash::vk;
use std::{fmt, io};

#[derive(Debug)]
pub(crate) enum BridgeError {
    Unsupported(&'static str),
    Invalid(&'static str),
    Busy,
    Retired,
    ImportOwnershipUncertain(vk::Result),
    Vk {
        stage: &'static str,
        result: vk::Result,
    },
    Io {
        stage: &'static str,
        source: io::Error,
    },
    Setup(String),
}

impl BridgeError {
    pub(super) fn vk(stage: &'static str, result: vk::Result) -> Self {
        Self::Vk { stage, result }
    }

    pub(super) fn io(stage: &'static str, source: impl Into<io::Error>) -> Self {
        Self::Io {
            stage,
            source: source.into(),
        }
    }
}

impl fmt::Display for BridgeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unsupported(reason) => write!(f, "unsupported Vulkan bridge: {reason}"),
            Self::Invalid(reason) => write!(f, "invalid Vulkan bridge input: {reason}"),
            Self::Busy => f.write_str("Vulkan bridge submission limit reached"),
            Self::Retired => f.write_str("Vulkan bridge is disabled or retired"),
            Self::ImportOwnershipUncertain(result) => write!(
                f,
                "DMA-BUF allocation failed ({result:?}); FD ownership uncertain, Vulkan bridge disabled until process exit"
            ),
            Self::Vk { stage, result } => write!(f, "{stage}: {result:?}"),
            Self::Io { stage, source } => write!(f, "{stage}: {source}"),
            Self::Setup(reason) => f.write_str(reason),
        }
    }
}

impl std::error::Error for BridgeError {}

/// A submitted failure retains its resources in the bridge. The caller must
/// retire both external allocations until polling proves completion; no immediate
/// fallback may read or overwrite those allocations.
#[derive(Debug)]
pub(crate) struct SubmitFailure {
    pub error: BridgeError,
    pub submitted: bool,
}

impl fmt::Display for SubmitFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} (possibly submitted: {})", self.error, self.submitted)
    }
}

impl std::error::Error for SubmitFailure {}
