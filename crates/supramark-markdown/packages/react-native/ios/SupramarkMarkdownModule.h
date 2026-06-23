/*
 * SupramarkMarkdownModule.h — RN native module header.
 *
 * Bridges JS `parseJson(source)` calls to the C ABI exported by
 * libsupramark_markdown_native.a:
 *
 *   int  supramark_markdown_parse_json(const char *input, size_t input_len,
 *                                      char **out_buf, size_t *out_len);
 *   void supramark_markdown_free(char *buf, size_t len);
 *   const char *supramark_markdown_version(void);
 *
 * See ../../include/supramark_markdown.h for the full contract.
 * Supports both old (RCTBridgeModule) and new (TurboModule) architectures.
 */

#import <React/RCTBridgeModule.h>

#ifdef RCT_NEW_ARCH_ENABLED
#import <SupramarkMarkdownNativeSpec/SupramarkMarkdownNativeSpec.h>
#endif

@interface SupramarkMarkdownModule : NSObject <RCTBridgeModule
#ifdef RCT_NEW_ARCH_ENABLED
  , NativeSupramarkMarkdownSpec
#endif
>

@end
