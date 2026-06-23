/*
 * Minimal <android/log.h> stub — TEST ONLY.
 *
 * The JNI bridge logs errors via __android_log_print. On a host JVM (no
 * Android NDK) that symbol/header is absent, so this stub routes the log
 * to stderr, letting supramark_markdown_jni.c compile and run unchanged
 * under scripts/test-android-jni.sh. It is NOT used in any real Android
 * build, which links the genuine NDK <android/log.h>.
 */
#ifndef SUPRAMARK_TEST_ANDROID_LOG_H
#define SUPRAMARK_TEST_ANDROID_LOG_H

#include <stdarg.h>
#include <stdio.h>

enum { ANDROID_LOG_ERROR = 6 };

static inline int __android_log_print(int prio, const char *tag, const char *fmt, ...) {
    (void)prio;
    fprintf(stderr, "[%s] ", tag ? tag : "");
    va_list ap;
    va_start(ap, fmt);
    int r = vfprintf(stderr, fmt, ap);
    va_end(ap);
    fputc('\n', stderr);
    return r;
}

#endif /* SUPRAMARK_TEST_ANDROID_LOG_H */
