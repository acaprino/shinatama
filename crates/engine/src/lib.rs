//! Headless SDK. Native launch admission belongs to C09R/C11, not this registry.
use serde::Serialize;
use shinatama_adapter::{
    AdapterDescriptor, AdapterError, AgentAdapter, AgentId, ConnectionArchitecture, ExportPage,
    ListRequest, Operation, PageLimits, ReadCursor, RefusalReason, SessionPage, TranscriptPage,
};
use std::collections::BTreeMap;

pub mod doctor;
pub mod checkup;
pub mod launch;
pub mod native_launch;
pub mod resources;
#[cfg(windows)]
pub mod claude_probe;
#[cfg(windows)]
pub mod claude_launch;
pub mod sidecar_config;
pub mod sidecar_runtime;
#[cfg(windows)]
pub mod sidecar_supervisor;

#[derive(Default)]
pub struct Engine {
    adapters: BTreeMap<AgentId, Box<dyn AgentAdapter>>,
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, adapter: Box<dyn AgentAdapter>) -> Result<(), RegistryError> {
        let descriptor = adapter.descriptor();
        if self.adapters.len() >= 32 {
            return Err(RegistryError::AdapterLimit);
        }
        if descriptor.synthetic
            != matches!(descriptor.architecture, ConnectionArchitecture::InMemory)
            || descriptor.synthetic != descriptor.agent.as_str().starts_with("fake_")
        {
            return Err(RegistryError::InvalidSyntheticIdentity);
        }
        let protocol = match &descriptor.architecture {
            ConnectionArchitecture::BackendAttach { protocol }
            | ConnectionArchitecture::StructuredBackend { protocol } => Some(protocol),
            _ => None,
        };
        if descriptor.display_name.is_empty()
            || descriptor.display_name.len() > 128
            || descriptor.display_name.chars().any(char::is_control)
            || protocol.is_some_and(|value| {
                value.is_empty() || value.len() > 128 || value.chars().any(char::is_control)
            })
        {
            return Err(RegistryError::InvalidDescriptor);
        }
        if self.adapters.contains_key(&descriptor.agent) {
            return Err(RegistryError::DuplicateAgent);
        }
        descriptor
            .capabilities
            .validate_for(&descriptor.agent, &descriptor.version, descriptor.synthetic)
            .map_err(|_| RegistryError::InvalidCapabilityEvidence)?;
        self.adapters.insert(descriptor.agent.clone(), adapter);
        Ok(())
    }

    pub fn descriptors(&self) -> Vec<&AdapterDescriptor> {
        self.adapters
            .values()
            .map(|adapter| adapter.descriptor())
            .collect()
    }
    pub fn descriptor(&self, id: &AgentId) -> Option<&AdapterDescriptor> {
        self.adapters.get(id).map(|a| a.descriptor())
    }

    pub fn require(&self, agent: &AgentId, operation: Operation) -> Result<(), AdapterError> {
        let adapter = self.adapter(agent, operation)?;
        let descriptor = adapter.descriptor();
        if &descriptor.agent != agent {
            return Err(AdapterError::new(operation, RefusalReason::VersionMismatch));
        }
        descriptor
            .capabilities
            .validate_for(agent, &descriptor.version, descriptor.synthetic)
            .map_err(|reason| AdapterError::new(operation, reason))?;
        // V1 cannot accidentally expose post-v1 native rewrite paths.
        if matches!(
            operation,
            Operation::ImportNative | Operation::VerifyImport | Operation::ForkNative
        ) {
            return Err(AdapterError::new(operation, RefusalReason::PostV1Only));
        }
        // Even a mistakenly declared capability cannot bypass process resilience.
        if matches!(
            operation,
            Operation::StartNew
                | Operation::ResumeNative
                | Operation::AttachBackend
                | Operation::AskHeadless
                | Operation::Login
                | Operation::InstallOrUpdate
        ) {
            return Err(AdapterError::new(
                operation,
                RefusalReason::NativeLaunchNeedsC09rAndC11,
            ));
        }
        adapter
            .descriptor()
            .capabilities
            .require(operation)
            .map_err(|reason| AdapterError::new(operation, reason))
    }

    pub fn list_sessions(
        &self,
        agent: &AgentId,
        request: &ListRequest,
    ) -> Result<SessionPage, AdapterError> {
        self.require(agent, Operation::ListSessions)?;
        request
            .validate()
            .map_err(|r| AdapterError::new(Operation::ListSessions, r))?;
        self.adapter(agent, Operation::ListSessions)?
            .list_sessions(request)
    }

    pub fn read_transcript(
        &self,
        agent: &AgentId,
        cursor: &ReadCursor,
        limits: PageLimits,
    ) -> Result<TranscriptPage, AdapterError> {
        self.require(agent, Operation::ReadTranscript)?;
        self.adapter(agent, Operation::ReadTranscript)?
            .read_transcript(cursor, limits)
    }

    pub fn export_interchange(
        &self,
        agent: &AgentId,
        cursor: &ReadCursor,
        limits: PageLimits,
    ) -> Result<ExportPage, AdapterError> {
        self.require(agent, Operation::ExportInterchange)?;
        self.adapter(agent, Operation::ExportInterchange)?
            .export_interchange(cursor, limits)
    }

    fn adapter(
        &self,
        agent: &AgentId,
        operation: Operation,
    ) -> Result<&dyn AgentAdapter, AdapterError> {
        self.adapters
            .get(agent)
            .map(|a| a.as_ref())
            .ok_or_else(|| AdapterError::new(operation, RefusalReason::AdapterNotRegistered))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistryError {
    DuplicateAgent,
    InvalidSyntheticIdentity,
    InvalidDescriptor,
    InvalidCapabilityEvidence,
    AdapterLimit,
}

#[cfg(test)]
mod tests {
    use super::*;
    use shinatama_adapter::fake::FakeAdapter;

    struct DeclaredAdapter(AdapterDescriptor);
    impl AgentAdapter for DeclaredAdapter {
        fn descriptor(&self) -> &AdapterDescriptor {
            &self.0
        }
    }

    #[test]
    fn zero_adapter_engine_refuses_instead_of_fabricating_sessions() {
        let engine = Engine::new();
        assert!(engine.descriptors().is_empty());
        let error = engine
            .list_sessions(
                &AgentId::new("codex").unwrap(),
                &ListRequest {
                    offset: 0,
                    limit: 10,
                    include_children: true,
                },
            )
            .unwrap_err();
        assert_eq!(error.reason, RefusalReason::AdapterNotRegistered);
    }

    #[test]
    fn fake_is_usable_but_no_native_launch_or_import_is_available() {
        let mut engine = Engine::new();
        engine
            .register(Box::new(FakeAdapter::new(vec![]).unwrap()))
            .unwrap();
        let agent = AgentId::new("fake_memory").unwrap();
        assert!(
            engine
                .list_sessions(
                    &agent,
                    &ListRequest {
                        offset: 0,
                        limit: 1,
                        include_children: true
                    }
                )
                .unwrap()
                .sessions
                .is_empty()
        );
        assert_eq!(
            engine
                .require(&agent, Operation::StartNew)
                .unwrap_err()
                .reason,
            RefusalReason::NativeLaunchNeedsC09rAndC11
        );
        assert_eq!(
            engine
                .require(&agent, Operation::ImportNative)
                .unwrap_err()
                .reason,
            RefusalReason::PostV1Only
        );
        assert_eq!(
            engine.register(Box::new(FakeAdapter::new(vec![]).unwrap())),
            Err(RegistryError::DuplicateAgent)
        );
    }

    #[test]
    fn registry_refuses_evidence_rebound_to_another_agent_or_build() {
        let fake = FakeAdapter::new(vec![]).unwrap();
        let descriptor = fake.descriptor().clone();
        let mut engine = Engine::new();
        let mut native = descriptor.clone();
        native.agent = AgentId::new("codex").unwrap();
        native.synthetic = false;
        native.architecture = ConnectionArchitecture::NativePty;
        assert_eq!(
            engine.register(Box::new(DeclaredAdapter(native))),
            Err(RegistryError::InvalidCapabilityEvidence)
        );
        let mut different_build = descriptor;
        different_build.version = shinatama_adapter::Version::parse("1.0.0+other").unwrap();
        assert_eq!(
            engine.register(Box::new(DeclaredAdapter(different_build))),
            Err(RegistryError::InvalidCapabilityEvidence)
        );
        assert!(engine.descriptors().is_empty());
    }

    #[test]
    fn operation_admission_rechecks_evidence_if_adapter_descriptor_changes() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        struct ChangingAdapter {
            original: AdapterDescriptor,
            changed: AdapterDescriptor,
            use_changed: Arc<AtomicBool>,
        }
        impl AgentAdapter for ChangingAdapter {
            fn descriptor(&self) -> &AdapterDescriptor {
                if self.use_changed.load(Ordering::Acquire) {
                    &self.changed
                } else {
                    &self.original
                }
            }
        }
        let original = FakeAdapter::new(vec![]).unwrap().descriptor().clone();
        let agent = original.agent.clone();
        let mut changed = original.clone();
        changed.version = shinatama_adapter::Version::parse("1.0.0+new-build").unwrap();
        let use_changed = Arc::new(AtomicBool::new(false));
        let mut engine = Engine::new();
        engine
            .register(Box::new(ChangingAdapter {
                original,
                changed,
                use_changed: use_changed.clone(),
            }))
            .unwrap();
        assert!(engine.require(&agent, Operation::ListSessions).is_ok());
        use_changed.store(true, Ordering::Release);
        assert_eq!(
            engine
                .require(&agent, Operation::ListSessions)
                .unwrap_err()
                .reason,
            RefusalReason::VersionMismatch
        );
    }
}
