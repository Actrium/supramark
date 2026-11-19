#import "SupramarkTextGeometry.h"
#import <React/RCTBridgeModule.h>
#import <React/RCTParagraphComponentView.h>
#import <react/renderer/textlayoutmanager/RCTAttributedTextUtils.h>

// Use the same content frame as the paragraph's drawRect:, including Yoga insets.
@interface RCTParagraphComponentView (SupramarkMeasurement)
- (CGRect)supramarkContentFrame;
@end
@implementation RCTParagraphComponentView (SupramarkMeasurement)
- (CGRect)supramarkContentFrame {
  const auto frame = _layoutMetrics.getContentFrame();
  return CGRectMake(frame.origin.x, frame.origin.y, frame.size.width, frame.size.height);
}
@end

@interface SupramarkSelectionMetrics : NSObject <RCTBridgeModule>
@end

@implementation SupramarkSelectionMetrics
RCT_EXPORT_MODULE()
@synthesize viewRegistry_DEPRECATED = _viewRegistry_DEPRECATED;
+ (BOOL)requiresMainQueueSetup { return NO; }
- (dispatch_queue_t)methodQueue { return dispatch_get_main_queue(); }

RCT_EXPORT_METHOD(measure:(NSNumber *)tag resolve:(RCTPromiseResolveBlock)resolve reject:(RCTPromiseRejectBlock)reject) {
  NSView *view = [self.viewRegistry_DEPRECATED viewForReactTag:tag];
  if (![view isKindOfClass:RCTParagraphComponentView.class]) {
    resolve(nil); // A recycled or detached paragraph has no current geometry.
    return;
  }
  RCTParagraphComponentView *paragraph = (RCTParagraphComponentView *)view;
  NSAttributedString *text = paragraph.attributedText;
  if (!text.length) {
    resolve(@[]);
    return;
  }
  NSTextStorage *storage = [[NSTextStorage alloc] initWithAttributedString:text];
  RCTApplyBaselineOffset(storage);
  resolve(SupramarkMeasureLines(storage, paragraph.supramarkContentFrame));
}
@end
