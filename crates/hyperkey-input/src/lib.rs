//! Win32 keyboard hook and modifier synthesis for Hyperkey.
//!
//! # Public API (`engine::InputEngine`)
//!
//! The app crate drives the engine through this surface:
//!
//! ```text
//! InputEngine::new(trigger_key: TriggerKey, output_modifiers: Vec<OutputModifier>) -> InputEngine
//! InputEngine::start(&self) -> bool
//! InputEngine::stop(&self)
//! InputEngine::configure(&self, trigger_key: TriggerKey, output_modifiers: Vec<OutputModifier>)
//! InputEngine::emergency_disable(&self)
//! InputEngine::reconcile(&self)
//! InputEngine::restart_hook(&self) -> bool
//! InputEngine::status(&self) -> InputEngineStatus
//! InputEngine::status_error(&self) -> Option<String>
//! InputEngine::last_recovery_message(&self) -> Option<String>
//! InputEngine::subscribe(&self) -> std::sync::mpsc::Receiver<EngineEvent>
//! impl Drop for InputEngine            // stops the engine
//! ```
//!
//! - `start` spawns the hook thread, installs the low-level keyboard hook,
//!   and starts the watchdog that probes the hook and reinstalls it when
//!   Windows silently removes it. Returns false unless the hook ends up
//!   `Running`.
//! - `stop` tears down the watchdog and the hook thread.
//! - `configure` swaps the trigger key and output modifiers at runtime; it
//!   releases any held generated modifiers first.
//! - `emergency_disable` resets the trigger machine and releases generated
//!   modifiers without persisting a disabled state.
//! - `reconcile` is the phase-3 recovery hook: identical reset, called by the
//!   app after sleep/resume, workstation lock/unlock, or a session change.
//! - `restart_hook` posts a forced reinstall request to the hook thread;
//!   returns false when the engine is not `Running` or the post fails.
//! - `status`/`status_error`/`last_recovery_message` read engine lifecycle
//!   state; a hook install or reinstall failure surfaces as
//!   `InputEngineStatus::Failed` with the reason in `status_error`.
//! - `subscribe` returns a channel receiver for every status transition
//!   (`EngineEvent::StatusChanged`) and every automatic hook recovery
//!   (`EngineEvent::HookRecovered`).
//!
//! `synth::ModifierSynthesizer` sends the generated modifier layer, trigger
//! tap replays, and watchdog liveness probes through `SendInput`.

pub mod engine;
pub mod synth;
