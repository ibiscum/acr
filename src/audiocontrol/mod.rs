// Audio controller module for managing multiple players
pub mod audiocontrol;
// EventBus for distributing PlayerEvents to subscribers
#[path = "event_bus.rs"]
pub mod event_bus;

// Re-export the AudioController
pub use audiocontrol::AudioController;
// Re-export the EventBus and related types
pub use event_bus::{EventBus, EventSubscription, EventSubscriber, SubscriberId};