//! Extra macro documentation.

#[doc(hidden)]
pub use fizyr_rpc_macros::interface as interface_impl;

#[macro_export]
/// Define an RPC interface.
///
/// This macro generates an `Interface, `Client` and `Server` struct, and some more helper types.
///
/// The `Interface` struct is used to enable RPC interface introspection.
/// Currently, it only supports retrieving the documentation of the interface itself.
/// In the future, you can use this struct to get a list of services and streaming messages too.
///
/// The client struct is used to initiate requests and send stream messages.
/// It can be created from a [`PeerWriteHandle`] or a [`PeerHandle`].
/// Note that if you create the client from a [`PeerHandle`], the [`PeerReadHandle`] will not be accessible.
///
/// For the server struct it is exactly the opposite: it is used to receive requests and stream messages.
/// It can be created from a [`PeerReadHandle`] or a [`PeerHandle`],
/// but creating it from a [`PeerHandle`] will discard the [`PeerWriteHandle`].
///
/// # Example
///
/// See the [`interface_example`] module for an example, with the source code and generated documentation.
///
/// # Syntax
///
/// The syntax for the macro is as follows:
/// ```no_compile
/// fizyr_rpc::interface! {
///     // The `interface` keyword defines an RPC interface.
///     // You must have exactly one interface definition in the macro invocation.
///     //
///     // The $interface_name is used in automatically generated documentation and should be UpperCamelCase.
///     //
///     // You can adjust the visiblity of the generated items with the `pub` keyword as normal.
///     // By default, all generated items are private.
///     //
///     // Each item can have user written documentation.
///     // Simply write doc comments with triple slashes as usual.
///     // This applies to the interface definition, service definitions, update definitions and stream definitions.
///     //
///     // The documentation writter on the `interface` item can be retrieved through the introspection API,
///     // but does not appear in rustdoc.
///     pub interface $interface_name {
///         // The `service` keyword defines a service.
///         //
///         // You can have any amount of service definitions inside an interface definition.
///         //
///         // The $id is used as the service ID and must be an i32.
///         // The ID must be unique for all services in the interface.
///         //
///         // The $name is the name of the service.
///         // It is used to generate function and type names.
///         // It must be a valid Rust identifier and should be lowercase with underscores.
///         //
///         // The $request_type and $response_type indicate the message body for the request and the response.
///         // If there is no data in a request or response, you can use the unit type: `()`
///         //
///         // If the service has no update messages, you can end the definition with a comma.
///         // See the next item for the syntax of services with update messages.
///         service $id $name: $request_type -> $response_type,
///
///         // If a service has update messages, you can declare them in the service block.
///         service $id $name: $request_type -> $response_type {
///             // The `request_update` keyword defines a request update.
///             // You can have any amount of request updates inside a service definition.
///             //
///             // The $id is used as the service ID for the update and must be an i32.
///             // The ID must be unique for all request updates in the service.
///             //
///             // The $name is the name of the update message.
///             // It is used to generate function and type names.
///             // It must be a valid Rust identifier and should be lowercase with underscores.
///             //
///             // The $body_type indicates the type of the message.
///             // If there is no data in the message, you can use the unit type: `()`
///             request_update $id $name: $body_type,
///
///             // The `response_update` keyword defines a response update.
///             // You can have any amount of response updates inside a service definition.
///             //
///             // The $id is used as the service ID for the update and must be an i32.
///             // The ID must be unique for all response updates in the service.
///             //
///             // The $name is the name of the update message.
///             // It is used to generate function and type names.
///             // It must be a valid Rust identifier and should be lowercase with underscores.
///             //
///             // The $body_type indicates the type of the message.
///             // If there is no data in the message, you can use the unit type: `()`
///             response_update $id $name: $body_type,
///         }
///
///         // The `stream` keyword defines a stream message.
///         // You can have any amount of stream definitions in an interface definition.
///         //
///         // The $id is used as the service ID of the stream message and must be an i32.
///         // The ID must be unique for all streams in the interface.
///         //
///         // The $name is the name of the stream.
///         // It is used to generate function and type names.
///         // It must be a valid Rust identifier and should be lowercase with underscores.
///         //
///         // The $body_type indicates the type of the message.
///         // If there is no data in the message, you can use the unit type: `()`
///        stream $id $name: $body_type,
///     }
/// }
/// ```
///
/// [`PeerHandle`]: crate::PeerHandle
/// [`PeerWriteHandle`]: crate::PeerWriteHandle
/// [`PeerReadHandle`]: crate::PeerReadHandle
macro_rules! interface {
	($($tokens:tt)*) => {
		$crate::macros::interface_impl!{$crate; $($tokens)*}
	}
}

/// Example module for the `interface!` macro.
///
/// You can compare the source of the generated documentation to inspect the generated API.
/// The most important generated types are [`Client`] and [`Server`].
///
/// [`Client`]: interface_example::Client
/// [`Server`]: interface_example::Server
///
/// ```no_compile
/// fizyr_rpc::interface! {
///     /// RPC interface for the supermarket.
///     pub interface Supermarket {
///         /// Greet the cashier.
///         ///
///         /// The cashier will reply with their own greeting.
///         service 1 greet_cashier: String -> String,
///
///         /// Purchase tomatoes.
///         ///
///         /// The response of the cashier depends on the update messages exchanged.
///         /// If you run away after they have sent the `price`, or if you pay with a nun-fungable token,
///         /// they will respond with [`BuyTomatoesResponse::ICalledSecurity`].
///         ///
///         /// If you pay with the correct amount, they will respond with [`BuyTomatoesResponse::ThankYouComeAgain`].
///         service 2 buy_tomatoes: BuyTomatoesRequest -> BuyTomatoesResponse {
///             /// Sent once by the cashier to notify you of the price of the tomatoes.
///             response_update 1 price: Price,
///
///             /// Sent by the client to pay for the tomatoes.
///             request_update 1 pay: Payment,
///
///             /// Sent by broke or kleptomanic clients that still want tomatoes.
///             request_update 2 run_away: (),
///         }
///
///         /// Mutter something as you're walking through the supermarket.
///         ///
///         /// Since no-one will respond, this is a stream rather than a service.
///         ///
///         /// Popular phrases include:
///         ///  * Woah thats cheap!
///         ///  * Everything used to be better in the good old days...
///         ///  * Why did they move the toilet paper?
///         stream 1 mutter: String,
///     }
/// }
///
/// /// The initial request to buy tomatoes.
/// #[derive(Debug)]
/// pub struct BuyTomatoesRequest {
///     /// The tomatoes you want to buy.
///     pub amount: usize,
/// }
///
/// /// The price for something.
/// #[derive(Debug)]
/// pub struct Price {
///     /// The total price in cents.
///     pub total_price_cents: usize,
/// }
///
/// /// Payment options for purchasing tomatoes.
/// #[derive(Debug)]
/// pub enum Payment {
///     /// Payment in money.
///     Money {
///         /// The amount of money in cents.
///         cents: usize
///     },
///
///     /// Payment with an NFT.
///     NonFungibleToken,
/// }
///
/// /// The response of a cashier when buying tomatoes.
/// #[derive(Debug)]
/// pub enum BuyTomatoesResponse {
///     /// A final greeting and your receipt.
///     ThankYouComeAgain(Receipt),
///
///     /// Security has been called.
///     ICalledSecurity,
/// }
///
/// /// A receipt for your purchase.
/// #[derive(Debug)]
/// pub struct Receipt {
///     /// The number of tomatoes you bought.
///     pub amount_of_tomatoes: usize,
///
///     /// The total price you paid for the tomatoes.
///     pub total_price_cents: usize,
///
///     /// If the cashier really liked you, they may write their phone number on the receipt with pen.
///     pub phone_number: Option<String>,
/// }
/// ```
pub mod interface_example {
	interface! {
		/// RPC interface for the supermarket.
		pub interface Supermarket {
			/// Greet the cashier.
			///
			/// The cashier will reply with their own greeting.
			service 1 greet_cashier: String -> String,

			/// Purchase tomatoes.
			///
			/// The response of the cashier depends on the update messages exchanged.
			/// If you run away after they have sent the `price`, or if you pay with a nun-fungable token,
			/// they will respond with [`BuyTomatoesResponse::ICalledSecurity`].
			///
			/// If you pay with the correct amount, they will respond with [`BuyTomatoesResponse::ThankYouComeAgain`].
			service 2 buy_tomatoes: BuyTomatoesRequest -> BuyTomatoesResponse {
				/// Sent once by the cashier to notify you of the price of the tomatoes.
				response_update 1 price: Price,

				/// Sent by the client to pay for the tomatoes.
				request_update 1 pay: Payment,

				/// Sent by broke or kleptomanic clients that still want tomatoes.
				request_update 2 run_away: (),
			}

			/// Mutter something as you're walking through the supermarket.
			///
			/// Since no-one will respond, this is a stream rather than a service.
			///
			/// Popular phrases include:
			///  * Woah thats cheap!
			///  * Everything used to be better in the good old days...
			///  * Why did they move the toilet paper?
			stream 1 mutter: String,
		}
	}

	/// The initial request to buy tomatoes.
	#[derive(Debug)]
	pub struct BuyTomatoesRequest {
		/// The tomatoes you want to buy.
		pub amount: usize,
	}

	/// The price for something.
	#[derive(Debug)]
	pub struct Price {
		/// The total price in cents.
		pub total_price_cents: usize,
	}

	/// Payment options for purchasing tomatoes.
	#[derive(Debug)]
	pub enum Payment {
		/// Payment in money.
		Money {
			/// The amount of money, in cents.
			cents: usize,
		},

		/// Payment with an NFT.
		NonFungibleToken,
	}

	/// The response of a cashier when buying tomatoes.
	#[derive(Debug)]
	pub enum BuyTomatoesResponse {
		/// A final greeting and your receipt.
		ThankYouComeAgain(Receipt),

		/// Security has been called.
		ICalledSecurity,
	}

	/// A receipt for your purchase.
	#[derive(Debug)]
	pub struct Receipt {
		/// The number of tomatoes you bought.
		pub amount_of_tomatoes: usize,

		/// The total price you paid for the tomatoes.
		pub total_price_cents: usize,

		/// If the cashier really liked you, they may write their phone number on the receipt with pen.
		pub phone_number: Option<String>,
	}
}
