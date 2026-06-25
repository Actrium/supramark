/*
 * GraphvizModuleTests.mm — XCTest for the iOS native bridge.
 *
 * Exercises the Objective-C marshalling layer of GraphvizModule end to
 * end: NSString DOT/engine/format -> the C FFI (gv_render via a lazily
 * created gv_context_t) -> NSString SVG, plus the promise resolve/reject
 * contract and the gv_error_t -> JS error-code mapping. This is the one
 * layer that the Rust unit tests and the JS adapter tests cannot reach,
 * because it only exists on the Apple platform.
 *
 * Run via scripts/test-ios.sh (builds against the prebuilt Graphviz
 * static lib + minimal RN header stubs, and runs it with `xcrun xctest`).
 */
#import <XCTest/XCTest.h>
#import "GraphvizModule.h"

/*
 * The bridge method is exported via RCT_EXPORT_METHOD in the .m, so RN
 * calls it through runtime introspection and it isn't declared in the
 * public header. Re-declare it here so the test can invoke it directly.
 * This changes nothing in production code.
 */
@interface GraphvizModule (Testing)
- (void)renderDot:(NSString *)dot
           engine:(NSString *)engine
           format:(NSString *)format
          resolve:(RCTPromiseResolveBlock)resolve
           reject:(RCTPromiseRejectBlock)reject;
- (void)getVersion:(RCTPromiseResolveBlock)resolve
            reject:(RCTPromiseRejectBlock)reject;
@end

@interface GraphvizModuleTests : XCTestCase
@end

@implementation GraphvizModuleTests {
    GraphvizModule *_module;
}

- (void)setUp {
    [super setUp];
    _module = [GraphvizModule new];
}

/// Helper: render with the "dot" engine to "svg" and return the resolved
/// string, failing the test if the bridge rejects instead.
- (NSString *)renderSVG:(NSString *)dot {
    XCTestExpectation *done = [self expectationWithDescription:@"renderDot settled"];
    __block NSString *resolved = nil;
    [_module renderDot:dot
                engine:@"dot"
                format:@"svg"
               resolve:^(id result) {
                   resolved = result;
                   [done fulfill];
               }
                reject:^(NSString *code, NSString *message, NSError *error) {
                    XCTFail(@"renderDot rejected unexpectedly: %@ / %@", code, message);
                    [done fulfill];
                }];
    [self waitForExpectations:@[ done ] timeout:30.0];
    return resolved;
}

/// A minimal directed graph renders to an SVG document. `svg` is one of
/// the bridge's text formats, so the result is returned as a raw NSString
/// (not base64) — a `<svg` substring proves the DOT -> FFI -> NSString
/// marshalling preserved the payload. (The output opens with an
/// `<?xml ?>` prologue ahead of the `<svg` root.)
- (void)testRenderSimpleDigraphReturnsSVG {
    NSString *svg = [self renderSVG:@"digraph { a -> b }"];
    XCTAssertNotNil(svg);
    XCTAssertGreaterThan(svg.length, 0u);
    XCTAssertTrue([svg containsString:@"<svg"],
                  @"expected an <svg root in the rendered output");
}

/// Syntactically invalid DOT is rejected with the documented INVALID_DOT
/// code (gv_render -> GV_ERR_INVALID_DOT). Verified directly against the
/// static lib: `gv_render(ctx, "this is not dot {{{", ...)` -> -2.
- (void)testInvalidDotRejectsInvalidDot {
    XCTestExpectation *done = [self expectationWithDescription:@"renderDot settled"];
    __block NSString *rejectCode = nil;
    __block BOOL resolved = NO;
    [_module renderDot:@"this is not dot {{{"
                engine:@"dot"
                format:@"svg"
               resolve:^(id result) {
                   resolved = YES;
                   [done fulfill];
               }
                reject:^(NSString *code, NSString *message, NSError *error) {
                    rejectCode = code;
                    [done fulfill];
                }];
    [self waitForExpectations:@[ done ] timeout:30.0];
    XCTAssertFalse(resolved, @"invalid DOT must not resolve");
    XCTAssertEqualObjects(rejectCode, @"INVALID_DOT");
}

/// A nil DOT source marshals to a NULL C string (`[nil UTF8String]`),
/// which gv_render reports as GV_ERR_NULL_INPUT, mapped by the bridge to
/// the `NULL_INPUT` reject code. Verified directly against the static
/// lib: `gv_render(ctx, NULL, ...)` -> -1.
- (void)testNilDotRejectsNullInput {
    XCTestExpectation *done = [self expectationWithDescription:@"renderDot settled"];
    __block NSString *rejectCode = nil;
    __block BOOL resolved = NO;
    [_module renderDot:nil
                engine:@"dot"
                format:@"svg"
               resolve:^(id result) {
                   resolved = YES;
                   [done fulfill];
               }
                reject:^(NSString *code, NSString *message, NSError *error) {
                    rejectCode = code;
                    [done fulfill];
                }];
    [self waitForExpectations:@[ done ] timeout:30.0];
    XCTAssertFalse(resolved, @"nil DOT must not resolve");
    XCTAssertEqualObjects(rejectCode, @"NULL_INPUT");
}

/// getVersion resolves with the linked Graphviz library's non-empty
/// version string (e.g. "14.1.5").
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
