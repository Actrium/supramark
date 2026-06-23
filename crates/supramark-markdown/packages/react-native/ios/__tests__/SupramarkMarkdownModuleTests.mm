/*
 * SupramarkMarkdownModuleTests.mm — XCTest for the iOS native bridge.
 *
 * Exercises the Objective-C marshalling layer of SupramarkMarkdownModule
 * end to end: NSString source -> NSData -> C FFI -> NSString JSON, plus
 * the promise resolve/reject contract. This is the one layer that the
 * Rust unit tests and the JS adapter tests cannot reach, because it only
 * exists on the Apple platform.
 *
 * Run via scripts/test-ios.sh (builds the host static lib, compiles this
 * bundle against minimal RN header stubs, and runs it with `xcrun xctest`).
 */
#import <XCTest/XCTest.h>
#import "SupramarkMarkdownModule.h"

/*
 * The bridge methods are exported via RCT_EXPORT_METHOD in the .mm, so RN
 * calls them through runtime introspection and they aren't declared in
 * the public header. Re-declare them here so the test can invoke them
 * directly. This changes nothing in production code.
 */
@interface SupramarkMarkdownModule (Testing)
- (void)parseJson:(NSString *)source
          resolve:(RCTPromiseResolveBlock)resolve
           reject:(RCTPromiseRejectBlock)reject;
- (void)getVersion:(RCTPromiseResolveBlock)resolve
            reject:(RCTPromiseRejectBlock)reject;
@end

@interface SupramarkMarkdownModuleTests : XCTestCase
@end

@implementation SupramarkMarkdownModuleTests {
    SupramarkMarkdownModule *_module;
}

- (void)setUp {
    [super setUp];
    _module = [SupramarkMarkdownModule new];
}

/// Helper: call parseJson and return the resolved JSON string, failing
/// the test if the bridge rejects instead.
- (NSString *)parse:(NSString *)source {
    XCTestExpectation *done = [self expectationWithDescription:@"parseJson settled"];
    __block NSString *resolved = nil;
    [_module parseJson:source
               resolve:^(id result) {
                   resolved = result;
                   [done fulfill];
               }
                reject:^(NSString *code, NSString *message, NSError *error) {
                    XCTFail(@"parseJson rejected unexpectedly: %@ / %@", code, message);
                    [done fulfill];
                }];
    [self waitForExpectations:@[ done ] timeout:5.0];
    return resolved;
}

/// A normal document round-trips to AST v2 JSON with a root node.
- (void)testParseSimpleReturnsRootJSON {
    NSString *json = [self parse:@"# Hello"];
    XCTAssertNotNil(json);

    NSData *data = [json dataUsingEncoding:NSUTF8StringEncoding];
    NSError *jsonErr = nil;
    NSDictionary *ast = [NSJSONSerialization JSONObjectWithData:data options:0 error:&jsonErr];
    XCTAssertNil(jsonErr, @"bridge must return valid JSON");
    XCTAssertEqualObjects(ast[@"type"], @"root");
}

/// Empty source is a valid empty document — the bridge must marshal an
/// empty NSString (length 0) through the FFI and resolve to a root node,
/// NOT reject.
///
/// Scope note: this guards the bridge-level behaviour. It does NOT, on a
/// macOS host, reproduce the specific failure the `input_len == 0` fix
/// targets, because `[@"" dataUsingEncoding:].bytes` returns a non-NULL
/// pointer here, so even the pre-fix strlen path happens to read it as an
/// empty C string. The fix matters where `bytes` is NULL (iOS device) or
/// where the buffer isn't NUL-terminated (Android JNI); the NULL/empty
/// FFI contract itself is teeth-tested in the Rust suite
/// (parse_empty_input_null_ptr / parse_empty_input_explicit_len).
- (void)testEmptyInputResolvesToEmptyRoot {
    NSString *json = [self parse:@""];
    XCTAssertNotNil(json, @"empty input must resolve, not reject");

    NSData *data = [json dataUsingEncoding:NSUTF8StringEncoding];
    NSDictionary *ast = [NSJSONSerialization JSONObjectWithData:data options:0 error:nil];
    XCTAssertEqualObjects(ast[@"type"], @"root");
}

/// A nil source is rejected synchronously with the documented code,
/// before any background dispatch.
- (void)testNilSourceRejectsNullInput {
    __block NSString *rejectCode = nil;
    __block BOOL resolved = NO;
    [_module parseJson:nil
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
