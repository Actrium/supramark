/*
 * SupramarkMermaidModuleTests.mm — XCTest for the iOS native bridge.
 *
 * Exercises the Objective-C marshalling layer of SupramarkMermaidModule end
 * to end: NSString source -> NSData -> C FFI (supramark_mermaid_render) ->
 * NSString SVG, plus the promise resolve/reject contract. This is the
 * one layer that the Rust unit tests and the JS adapter tests cannot
 * reach, because it only exists on the Apple platform.
 *
 * Run via scripts/test-ios.sh (builds the host static lib, compiles this
 * bundle against minimal RN header stubs, and runs it with `xcrun xctest`).
 */
#import <XCTest/XCTest.h>
#import "SupramarkMermaidModule.h"

/*
 * The bridge methods are exported via RCT_EXPORT_METHOD in the .mm, so RN
 * calls them through runtime introspection and they aren't declared in
 * the public header. Re-declare them here so the test can invoke them
 * directly. This changes nothing in production code.
 */
@interface SupramarkMermaidModule (Testing)
- (void)render:(NSString *)source
       resolve:(RCTPromiseResolveBlock)resolve
        reject:(RCTPromiseRejectBlock)reject;
- (void)getVersion:(RCTPromiseResolveBlock)resolve
            reject:(RCTPromiseRejectBlock)reject;
@end

@interface SupramarkMermaidModuleTests : XCTestCase
@end

@implementation SupramarkMermaidModuleTests {
    SupramarkMermaidModule *_module;
}

- (void)setUp {
    [super setUp];
    _module = [SupramarkMermaidModule new];
}

/// Helper: call render and return the resolved SVG string, failing the
/// test if the bridge rejects instead.
- (NSString *)render:(NSString *)source {
    XCTestExpectation *done = [self expectationWithDescription:@"render settled"];
    __block NSString *resolved = nil;
    [_module render:source
            resolve:^(id result) {
                resolved = result;
                [done fulfill];
            }
             reject:^(NSString *code, NSString *message, NSError *error) {
                 XCTFail(@"render rejected unexpectedly: %@ / %@", code, message);
                 [done fulfill];
             }];
    [self waitForExpectations:@[ done ] timeout:30.0];
    return resolved;
}

/// A minimal flowchart round-trips to an SVG document. `flowchart TD\nA-->B`
/// is the same source the native crate's Rust round-trip test uses, so a
/// `<svg` substring proves the NSString -> FFI -> NSString marshalling
/// preserved the payload.
- (void)testRenderSimpleReturnsSVG {
    NSString *svg = [self render:@"flowchart TD\nA-->B"];
    XCTAssertNotNil(svg);
    XCTAssertGreaterThan(svg.length, 0u);
    XCTAssertTrue([svg containsString:@"<svg"],
                  @"expected an <svg root in the rendered output");
}

/// Empty source is NOT a valid Mermaid diagram (unlike D2 / PlantUML):
/// the parser has no diagram type to dispatch on, so the FFI returns
/// SUPRAMARK_MERMAID_ERR_PARSE (verified directly against the static
/// lib: `supramark_mermaid_render("", 0, ...)` -> rc 1). This test pins
/// that the bridge marshals the empty NSString (non-nil NSData of length
/// 0) into the FFI and surfaces the documented `PARSE_ERROR` reject —
/// not a spurious NULL_INPUT, and not a resolve.
- (void)testEmptyInputRejectsParseError {
    XCTestExpectation *done = [self expectationWithDescription:@"render settled"];
    __block NSString *rejectCode = nil;
    __block BOOL resolved = NO;
    [_module render:@""
            resolve:^(id result) {
                resolved = YES;
                [done fulfill];
            }
             reject:^(NSString *code, NSString *message, NSError *error) {
                 rejectCode = code;
                 [done fulfill];
             }];
    [self waitForExpectations:@[ done ] timeout:30.0];
    XCTAssertFalse(resolved, @"empty Mermaid source must not resolve");
    XCTAssertEqualObjects(rejectCode, @"PARSE_ERROR");
}

/// A nil source is rejected synchronously with the documented code,
/// before any background dispatch.
- (void)testNilSourceRejectsNullInput {
    __block NSString *rejectCode = nil;
    __block BOOL resolved = NO;
    [_module render:nil
            resolve:^(id result) { resolved = YES; }
             reject:^(NSString *code, NSString *message, NSError *error) {
                 rejectCode = code;
             }];
    XCTAssertFalse(resolved);
    XCTAssertEqualObjects(rejectCode, @"NULL_INPUT");
}

/// getVersion resolves with the linked library's non-empty version string.
- (void)testGetVersionResolves {
    XCTestExpectation *done = [self expectationWithDescription:@"getVersion settled"];
    __block NSString *version = nil;
    [_module getVersion:^(id result) {
        version = result;
        [done fulfill];
    }
                 reject:^(NSString *code, NSString *message, NSError *error) {
                     XCTFail(@"getVersion rejected: %@ / %@", code, message);
                     [done fulfill];
                 }];
    [self waitForExpectations:@[ done ] timeout:5.0];
    XCTAssertGreaterThan(version.length, 0u);
}

@end
