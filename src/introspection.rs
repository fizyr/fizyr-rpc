//! Support types and traits for runtime interface instrospection.
//!
//! These types and traits are used by generated interfaces from the [`interface!`] macro.
//! Normally, you would only implement the traits for your own serialization format.
//! However, the traits are covered by semver guarantees, so feel free to use them in your own code.

/// Metadata about an RPC interface for runtime introspection.
#[derive(Debug, Clone)]
pub struct InterfaceDefinition<TypeInfo> {
	/// The name of the interface.
	pub name: String,

	/// The documentation of the interface.
	///
	/// This string may contain rustdoc compatible markup.
	pub doc: String,

	/// The list of services in the interface.
	pub services: Vec<ServiceDefinition<TypeInfo>>,

	/// The list of streams in the interface.
	pub streams: Vec<StreamDefinition<TypeInfo>>,
}

/// Metadata about a service for runtime intropection.
#[derive(Debug, Clone)]
pub struct ServiceDefinition<TypeInfo> {
	/// The name of the service.
	pub name: String,

	/// The documentation of the service.
	///
	/// This string may contain rustdoc compatible markup.
	pub doc: String,

	/// The service ID of the service.
	pub service_id: i32,

	/// Information about the request body.
	pub request_body: TypeInfo,

	/// Information about the response body.
	pub response_body: TypeInfo,

	/// Information about the request updates.
	pub request_updates: Vec<UpdateDefinition<TypeInfo>>,

	/// Information about the response updates.
	pub response_updates: Vec<UpdateDefinition<TypeInfo>>,
}

/// Metadata about a service update for runtime intropection.
#[derive(Debug, Clone)]
pub struct UpdateDefinition<TypeInfo> {
	/// The name of the update message.
	pub name: String,

	/// The documentation of the update message.
	///
	/// This string may contain rustdoc compatible markup.
	pub doc: String,

	/// The service ID of the update message.
	pub service_id: i32,

	/// Information about the message body.
	pub body: TypeInfo,
}

/// Metadata about a stream message for runtime intropection.
#[derive(Debug, Clone)]
pub struct StreamDefinition<TypeInfo> {
	/// The name of the stream message.
	pub name: String,

	/// The documentation of the stream message.
	///
	/// This string may contain rustdoc compatible markup.
	pub doc: String,

	/// The service ID of the stream message.
	pub service_id: i32,

	/// Information about the message body.
	pub body: TypeInfo,
}

/// Trait for formats that can provide runtime type information.
pub trait IntrospectableFormat: crate::format::Format {
	/// The return type for the [`Self::type_info()`] function.
	type TypeInfo;
}

/// Trait for formats to provide runtime type information about a message body.
pub trait FormatTypeInfo<T: ?Sized>: IntrospectableFormat {
	/// Get type information about a type.
	fn type_info() -> Self::TypeInfo;
}
