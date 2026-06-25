/*
 * SupramarkD2ModuleTests.mm — XCTest for the iOS native bridge.
 *
 * Exercises the Objective-C marshalling layer of SupramarkD2Module end
 * to end: NSString source -> NSData -> C FFI (supramark_d2_render) ->
 * NSString SVG, plus the promise resolve/reject contract. This is the
 * one layer that the Rust unit tests and the JS adapter tests cannot
 * reach, because it only exists on the Apple platform.
 *
 * Run via scripts/test-ios.sh (builds the host static lib, compiles this
 * bundle against minimal RN header stubs, and runs it with `xcrun xctest`).
 */
#import <XCTest/XCTest.h>
#import "SupramarkD2Module.h"

/*
 * The bridge methods are exported via RCT_EXPORT_METHOD in the .mm, so RN
 * calls them through runtime introspection and they aren't declared in
 * the public header. Re-declare them here so the test can invoke them
 * directly. This changes nothing in production code.
 */
@interface SupramarkD2Module (Testing)
- (void)render:(NSString *)source
       resolve:(RCTPromiseResolveBlock)resolve
        reject:(RCTPromiseRejectBlock)reject;
- (void)getVersion:(RCTPromiseResolveBlock)resolve
            reject:(RCTPromiseRejectBlock)reject;
@end

@interface SupramarkD2ModuleTests : XCTestCase
@end

@implementation SupramarkD2ModuleTests {
    SupramarkD2Module *_module;
}

- (void)setUp {
    [super setUp];
    _module = [SupramarkD2Module new];
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

/// The smallest meaningful D2 diagram round-trips to an SVG document.
/// `a -> b` is the same source the native crate's Rust round-trip test
/// uses, so a `<svg` substring proves the NSString -> FFI -> NSString
/// marshalling preserved the payload.
- (void)testRenderSimpleReturnsSVG {
    NSString *svg = [self render:@"a -> b"];
    XCTAssertNotNil(svg);
    XCTAssertGreaterThan(svg.length, 0u);
    XCTAssertTrue([svg containsString:@"<svg"],
                  @"expected an <svg root in the rendered output");
}

/// Empty source is a valid empty D2 document. The bridge must marshal an
/// empty NSString (length 0) through the FFI and resolve to a (possibly
/// empty) SVG string, NOT reject. This guards the bridge-level NULL /
/// empty contract: `[@"" dataUsingEncoding:]` yields a non-nil NSData of
/// length 0, and the FFI's `input_len == 0` path treats it as the empty
/// diagram.
- (void)testEmptyInputResolvesNotReject {
    NSString *svg = [self render:@""];
    XCTAssertNotNil(svg, @"empty input must resolve, not reject");
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
