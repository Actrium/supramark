/*
 * SupramarkPlantumlModuleTests.mm — XCTest for the iOS native bridge.
 *
 * Exercises the Objective-C marshalling layer of SupramarkPlantumlModule end
 * to end: NSString source -> NSData -> C FFI (supramark_plantuml_render) ->
 * NSString SVG, plus the promise resolve/reject contract. This is the
 * one layer that the Rust unit tests and the JS adapter tests cannot
 * reach, because it only exists on the Apple platform.
 *
 * Run via scripts/test-ios.sh (builds the host static lib, compiles this
 * bundle against minimal RN header stubs, and runs it with `xcrun xctest`).
 */
#import <XCTest/XCTest.h>
#import "SupramarkPlantumlModule.h"

/*
 * The bridge methods are exported via RCT_EXPORT_METHOD in the .mm, so RN
 * calls them through runtime introspection and they aren't declared in
 * the public header. Re-declare them here so the test can invoke them
 * directly. This changes nothing in production code.
 */
@interface SupramarkPlantumlModule (Testing)
- (void)render:(NSString *)source
       resolve:(RCTPromiseResolveBlock)resolve
        reject:(RCTPromiseRejectBlock)reject;
- (void)getVersion:(RCTPromiseResolveBlock)resolve
            reject:(RCTPromiseRejectBlock)reject;
@end

@interface SupramarkPlantumlModuleTests : XCTestCase
@end

@implementation SupramarkPlantumlModuleTests {
    SupramarkPlantumlModule *_module;
}

- (void)setUp {
    [super setUp];
    _module = [SupramarkPlantumlModule new];
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

/// A minimal sequence diagram round-trips to an SVG document.
/// `@startuml\nA -> B\n@enduml` is the same source the native crate's
/// Rust round-trip test uses, so a `<svg` substring proves the NSString
/// -> FFI -> NSString marshalling preserved the payload. (The output is
/// prefixed with a `<?plantuml ?>` prologue ahead of the `<svg` root.)
///
/// The bridge installs no font-metrics callback, so layout falls back to
/// the `size * 0.6`-per-char heuristic — the diagram still renders.
- (void)testRenderSimpleReturnsSVG {
    NSString *svg = [self render:@"@startuml\nA -> B\n@enduml"];
    XCTAssertNotNil(svg);
    XCTAssertGreaterThan(svg.length, 0u);
    XCTAssertTrue([svg containsString:@"<svg"],
                  @"expected an <svg root in the rendered output");
}

/// An empty source has no `@startuml/@enduml` block, so the PlantUML
/// pipeline produces no diagram and the FFI returns
/// SUPRAMARK_PLANTUML_ERR_RENDER (verified directly against the static
/// lib: `supramark_plantuml_render("", 0, ...)` -> rc 2). The bridge
/// must surface that as the documented `RENDER_ERROR` reject — not a
/// NULL_INPUT (the NSData is non-nil, length 0) and not a resolve.
- (void)testEmptyInputRejectsRenderError {
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
    XCTAssertFalse(resolved, @"empty PlantUML source must not resolve");
    XCTAssertEqualObjects(rejectCode, @"RENDER_ERROR");
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
