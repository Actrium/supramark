/*
 * Minimal React Native header stub — TEST ONLY.
 *
 * `SupramarkMarkdownModule` is a self-contained bridge: its exported
 * methods marshal NSString <-> the C FFI and report results through the
 * promise blocks, without touching `self.bridge` or any RN runtime
 * injection. So the marshalling layer can be unit tested against the
 * documented RN block contract without linking the full React Native
 * framework (which would require a CocoaPods install of React-Core).
 *
 * These typedefs / macros mirror the public shapes from
 * React/RCTBridgeModule.h closely enough to compile and exercise the
 * module exactly as RN would call it. They are NOT a reimplementation of
 * RN — only what the module under test references.
 */
#import <Foundation/Foundation.h>

typedef void (^RCTPromiseResolveBlock)(id result);
typedef void (^RCTPromiseRejectBlock)(NSString *code, NSString *message, NSError *error);

@protocol RCTBridgeModule <NSObject>
@end

/*
 * In real RN these register the module with the bridge. The unit test
 * instantiates the module directly, so registration is a no-op here and
 * the method macro just emits a plain Objective-C instance method.
 */
#define RCT_EXPORT_MODULE(js_name)
#define RCT_EXPORT_METHOD(...) -(void)__VA_ARGS__
