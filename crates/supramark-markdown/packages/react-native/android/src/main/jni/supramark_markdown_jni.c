/*
 * supramark_markdown_jni.c — JNI bridge between Java SupramarkMarkdownModule
 * and the C ABI exported by libsupramark_markdown_native.so.
 *
 * Function naming follows JNI mangling for class
 * com.supramark.markdownnative.SupramarkMarkdownModule — static native
 * methods `nativeParseJson(byte[]) -> byte[]` and `nativeGetVersion() -> String`.
 */

#include <jni.h>
#include <stdint.h>
#include <android/log.h>

#include "supramark_markdown.h"

#define TAG "SupramarkMarkdownJNI"
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, TAG, __VA_ARGS__)

JNIEXPORT jbyteArray JNICALL
Java_com_supramark_markdownnative_SupramarkMarkdownModule_nativeParseJson(
    JNIEnv *env, jclass cls, jbyteArray jSourceUtf8, jintArray jStatusOut)
{
    (void)cls;
    if (jSourceUtf8 == NULL) {
        if (jStatusOut != NULL) {
            jint err = SUPRAMARK_MARKDOWN_ERR_NULL_INPUT;
            (*env)->SetIntArrayRegion(env, jStatusOut, 0, 1, &err);
        }
        return NULL;
    }

    jsize srcLen = (*env)->GetArrayLength(env, jSourceUtf8);
    jbyte *src = (*env)->GetByteArrayElements(env, jSourceUtf8, NULL);
    if (src == NULL) {
        if (jStatusOut != NULL) {
            jint err = SUPRAMARK_MARKDOWN_ERR_NULL_INPUT;
            (*env)->SetIntArrayRegion(env, jStatusOut, 0, 1, &err);
        }
        return NULL;
    }

    char *outBuf = NULL;
    size_t outLen = 0;
    int status = supramark_markdown_parse_json((const char *)src, (size_t)srcLen, &outBuf, &outLen);
    (*env)->ReleaseByteArrayElements(env, jSourceUtf8, src, JNI_ABORT);

    if (jStatusOut != NULL) {
        jint s = status;
        (*env)->SetIntArrayRegion(env, jStatusOut, 0, 1, &s);
    }

    if (status != SUPRAMARK_MARKDOWN_OK) {
        LOGE("supramark_markdown_parse_json returned %d", status);
        return NULL;
    }

    // JNI array lengths are signed 32-bit, so reject impossible JS payload sizes.
    if (outLen > (size_t)INT32_MAX) {
        supramark_markdown_free(outBuf, outLen);
        if (jStatusOut != NULL) {
            jint err = SUPRAMARK_MARKDOWN_ERR_SERIALIZE;
            (*env)->SetIntArrayRegion(env, jStatusOut, 0, 1, &err);
        }
        return NULL;
    }

    jbyteArray result = (*env)->NewByteArray(env, (jsize)outLen);
    // Allocation failure is reported as a serialization failure to JS.
    if (result == NULL) {
        supramark_markdown_free(outBuf, outLen);
        if (jStatusOut != NULL) {
            jint err = SUPRAMARK_MARKDOWN_ERR_SERIALIZE;
            (*env)->SetIntArrayRegion(env, jStatusOut, 0, 1, &err);
        }
        return NULL;
    }

    // Return standard UTF-8 bytes to Java; Java owns decoding into a String.
    (*env)->SetByteArrayRegion(env, result, 0, (jsize)outLen, (const jbyte *)outBuf);
    supramark_markdown_free(outBuf, outLen);
    return result;
}

JNIEXPORT jstring JNICALL
Java_com_supramark_markdownnative_SupramarkMarkdownModule_nativeGetVersion(
    JNIEnv *env, jclass cls)
{
    (void)cls;
    const char *v = supramark_markdown_version();
    if (v == NULL) return NULL;
    return (*env)->NewStringUTF(env, v);
}
