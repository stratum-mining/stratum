use binary_sv2::Seq064K;
use extensions_sv2::{RequestExtensions, RequestExtensionsError, RequestExtensionsSuccess};

fn main() {
    println!("=== Extensions Negotiation Example ===\n");

    // Example 1: Client requests support for extension 0x0001
    println!("1. Client requests extension [0x0001]:");
    let requested = vec![0x0001_u16];
    let request = RequestExtensions {
        request_id: 1,
        requested_extensions: Seq064K::new(requested.clone()).unwrap(),
    };
    println!("   {}", request);
    println!("   Request ID: {}", request.request_id);
    let inner = request.requested_extensions.into_inner();
    println!("   Requested extensions: {:?}\n", inner);

    // Example 2: Server accepts the extension (0x0001)
    println!("2. Server accepts extension [0x0001]:");
    let supported = vec![0x0001_u16];
    let success = RequestExtensionsSuccess {
        request_id: 1,
        supported_extensions: Seq064K::new(supported.clone()).unwrap(),
    };
    println!("   {}", success);
    println!("   Request ID: {}", success.request_id);
    let inner = success.supported_extensions.into_inner();
    println!("   Supported extensions: {:?}\n", inner);

    // Example 3: Server rejects some extensions and requires others
    println!("3. Server rejects [0x0003] and requires [0x0005]:");
    let unsupported = vec![0x0003_u16];
    let required = vec![0x0005_u16];
    let error = RequestExtensionsError {
        request_id: 1,
        unsupported_extensions: Seq064K::new(unsupported.clone()).unwrap(),
        required_extensions: Seq064K::new(required.clone()).unwrap(),
    };
    println!("   {}", error);
    println!("   Request ID: {}", error.request_id);
    let unsupported_inner = error.unsupported_extensions.into_inner();
    let required_inner = error.required_extensions.into_inner();
    println!("   Unsupported extensions: {:?}", unsupported_inner);
    println!("   Required extensions: {:?}\n", required_inner);

    // Example 4: Demonstrate message types and extension type
    println!("4. Protocol constants:");
    println!(
        "   Extension type (Extensions Negotiation): 0x{:04x}",
        extensions_sv2::EXTENSION_TYPE_EXTENSIONS_NEGOTIATION
    );
    println!(
        "   Message type (RequestExtensions): 0x{:02x}",
        extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS
    );
    println!(
        "   Message type (RequestExtensions.Success): 0x{:02x}",
        extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_SUCCESS
    );
    println!(
        "   Message type (RequestExtensions.Error): 0x{:02x}",
        extensions_sv2::MESSAGE_TYPE_REQUEST_EXTENSIONS_ERROR
    );

    println!("\n=== Example Complete ===");
}
