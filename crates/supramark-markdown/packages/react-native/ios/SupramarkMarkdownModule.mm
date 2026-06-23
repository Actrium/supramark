/*
 * SupramarkMarkdownModule.mm — RN native module impl (iOS/macOS).
 *
 * Pulls Markdown source from JS, dispatches to a serial background
 * queue, calls supramark_markdown_parse_json, and resolves the Promise
 * with an UTF-8 AST v2 JSON string.
 */

#import "SupramarkMarkdownModule.h"
#import "supramark_markdown.h"

#import <React/RCTLog.h>

@implementation SupramarkMarkdownModule {
    dispatch_queue_t _parseQueue;
}

RCT_EXPORT_MODULE(SupramarkMarkdownNative)

- (instancetype)init {
    self = [super init];
    if (self) {
        _parseQueue = dispatch_queue_create("com.supramark.markdownnative.parse",
                                            DISPATCH_QUEUE_SERIAL);
    }
    return self;
}

+ (BOOL)requiresMainQueueSetup {
    return NO;
}

#pragma mark - parseJson(source) -> Promise<string>

RCT_EXPORT_METHOD(parseJson:(NSString *)source
                  resolve:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)
{
    if (source == nil) {
        reject(@"NULL_INPUT", @"parseJson: source is null", nil);
        return;
    }
    NSData *sourceData = [source dataUsingEncoding:NSUTF8StringEncoding];
    if (sourceData == nil) {
        reject(@"NULL_INPUT", @"parseJson: source is not valid UTF-8", nil);
        return;
    }

    dispatch_async(_parseQueue, ^{
        char *outBuf = NULL;
        size_t outLen = 0;
        int32_t status = supramark_markdown_parse_json((const char *)[sourceData bytes],
                                                        [sourceData length],
                                                        &outBuf,
                                                        &outLen);
        if (status != SUPRAMARK_MARKDOWN_OK) {
            NSString *code;
            switch (status) {
                case SUPRAMARK_MARKDOWN_ERR_SERIALIZE:  code = @"SERIALIZE_ERROR"; break;
                case SUPRAMARK_MARKDOWN_ERR_NULL_INPUT: code = @"NULL_INPUT"; break;
                default:                                 code = @"UNKNOWN"; break;
            }
            reject(code,
                   [NSString stringWithFormat:@"supramark_markdown_parse_json returned %d", status],
                   nil);
            return;
        }
        // Copy the buffer into NSString before freeing.
        NSString *json = [[NSString alloc] initWithBytes:outBuf
                                                  length:outLen
                                                encoding:NSUTF8StringEncoding];
        supramark_markdown_free(outBuf, outLen);
        if (json == nil) {
            reject(@"SERIALIZE_ERROR",
                   @"supramark_markdown_parse_json returned bytes that aren't valid UTF-8",
                   nil);
            return;
        }
        resolve(json);
    });
}

#pragma mark - getVersion() -> Promise<string>

RCT_EXPORT_METHOD(getVersion:(RCTPromiseResolveBlock)resolve
                  reject:(RCTPromiseRejectBlock)reject)
{
    const char *v = supramark_markdown_version();
    if (v == NULL) {
        reject(@"UNKNOWN", @"supramark_markdown_version returned NULL", nil);
        return;
    }
    resolve([NSString stringWithUTF8String:v]);
}

#ifdef RCT_NEW_ARCH_ENABLED
- (std::shared_ptr<facebook::react::TurboModule>)getTurboModule:
    (const facebook::react::ObjCTurboModule::InitParams &)params
{
    return std::make_shared<facebook::react::NativeSupramarkMarkdownSpecJSI>(params);
}
#endif

@end
