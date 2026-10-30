use crate::keys::TriggerKey;
use std::collections::HashSet;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum OutputModifier {
    Control,
    Alt,
    Shift,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum TapBehavior {
    CapsLock,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct HyperkeySettings {
    schema_version: i32,
    enabled: bool,
    trigger: TriggerKey,
    output_modifiers: Vec<OutputModifier>,
    launch_at_startup: bool,
    launch_to_tray: bool,
    tap_behavior: TapBehavior,
}

impl HyperkeySettings {
    pub const CURRENT_SCHEMA_VERSION: i32 = 2;

    pub fn defaults() -> Self {
        Self::create(
            true,
            TriggerKey::CAPS_LOCK,
            vec![
                OutputModifier::Control,
                OutputModifier::Alt,
                OutputModifier::Shift,
            ],
            true,
            false,
            TapBehavior::CapsLock,
        )
        .expect("the compiled-in default settings satisfy every invariant")
    }

    pub fn create(
        enabled: bool,
        trigger: TriggerKey,
        output_modifiers: Vec<OutputModifier>,
        launch_at_startup: bool,
        launch_to_tray: bool,
        tap_behavior: TapBehavior,
    ) -> Result<Self, String> {
        Self::create_with_schema_version(
            enabled,
            trigger,
            output_modifiers,
            launch_at_startup,
            launch_to_tray,
            tap_behavior,
            Self::CURRENT_SCHEMA_VERSION,
        )
    }

    pub fn create_with_schema_version(
        enabled: bool,
        trigger: TriggerKey,
        output_modifiers: Vec<OutputModifier>,
        launch_at_startup: bool,
        launch_to_tray: bool,
        tap_behavior: TapBehavior,
        schema_version: i32,
    ) -> Result<Self, String> {
        if !(1..=Self::CURRENT_SCHEMA_VERSION).contains(&schema_version) {
            return Err(format!("Unsupported schema version: {schema_version}."));
        }

        if !trigger.is_supported() {
            return Err(format!("Unsupported trigger key: {}.", trigger.wire_name()));
        }

        if tap_behavior != TapBehavior::CapsLock {
            return Err("Unsupported tap behavior.".to_string());
        }

        if output_modifiers.is_empty()
            || output_modifiers.len() > Self::defaults_output_modifiers().len()
        {
            return Err(
                "Choose at least one and no more than three output modifiers.".to_string(),
            );
        }

        if output_modifiers
            .iter()
            .collect::<HashSet<_>>()
            .len()
            != output_modifiers.len()
        {
            return Err("Output modifiers must be unique.".to_string());
        }

        Ok(Self {
            schema_version,
            enabled,
            trigger,
            output_modifiers,
            launch_at_startup,
            launch_to_tray,
            tap_behavior,
        })
    }

    pub fn schema_version(&self) -> i32 {
        self.schema_version
    }

    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn trigger(&self) -> TriggerKey {
        self.trigger
    }

    pub fn output_modifiers(&self) -> &[OutputModifier] {
        &self.output_modifiers
    }

    pub fn launch_at_startup(&self) -> bool {
        self.launch_at_startup
    }

    pub fn launch_to_tray(&self) -> bool {
        self.launch_to_tray
    }

    pub fn tap_behavior(&self) -> TapBehavior {
        self.tap_behavior
    }

    pub fn with_enabled(&self, enabled: bool) -> Self {
        Self {
            enabled,
            ..self.clone()
        }
    }

    pub fn with_launch_at_startup(&self, launch_at_startup: bool) -> Self {
        Self {
            launch_at_startup,
            ..self.clone()
        }
    }

    pub fn with_launch_to_tray(&self, launch_to_tray: bool) -> Self {
        Self {
            launch_to_tray,
            ..self.clone()
        }
    }

    pub fn with_trigger(&self, trigger: TriggerKey) -> Result<Self, String> {
        Self::create_with_schema_version(
            self.enabled,
            trigger,
            self.output_modifiers.clone(),
            self.launch_at_startup,
            self.launch_to_tray,
            self.tap_behavior,
            self.schema_version,
        )
    }

    pub fn with_output_modifiers(
        &self,
        output_modifiers: Vec<OutputModifier>,
    ) -> Result<Self, String> {
        Self::create_with_schema_version(
            self.enabled,
            self.trigger,
            output_modifiers,
            self.launch_at_startup,
            self.launch_to_tray,
            self.tap_behavior,
            self.schema_version,
        )
    }

    fn defaults_output_modifiers() -> [OutputModifier; 3] {
        [
            OutputModifier::Control,
            OutputModifier::Alt,
            OutputModifier::Shift,
        ]
    }
}
