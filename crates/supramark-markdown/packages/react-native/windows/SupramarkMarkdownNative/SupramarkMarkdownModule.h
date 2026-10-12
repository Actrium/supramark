/*
 * SupramarkMarkdownModule.h (Windows)
 *
 * C++/WinRT React Native module for Markdown parsing on Windows.
 * Bridges JS parseJson(source) calls to the C ABI exported by
 * supramark_markdown_native.dll:
 *
 *   int32_t supramark_markdown_parse_json(const char* input, size_t input_len,
 *                                         char** out_buf, size_t* out_len);
 *   void    supramark_markdown_free(char* buf, size_t len);
 *   const char* supramark_markdown_version(void);
 *
 * Licensed under the Apache License, Version 2.0
 */

#pragma once

#include <string>
#include <thread>

#include <NativeModules.h>

extern "C" {
#include "supramark_markdown.h"
}

namespace winrt::SupramarkMarkdownNative::implementation {

REACT_MODULE(SupramarkMarkdownModule, L"SupramarkMarkdownNative")
struct SupramarkMarkdownModule {

    REACT_METHOD(parseJson, L"parseJson")
    void parseJson(std::string source, React::ReactPromise<std::string> promise) noexcept {
        // Dispatch to a worker thread to avoid blocking the JS thread.
        std::thread([this, source = std::move(source), promise = std::move(promise)]() mutable {
            char *outBuf = nullptr;
            size_t outLen = 0;

            int32_t status = supramark_markdown_parse_json(
                source.c_str(), source.size(), &outBuf, &outLen);

            if (status != SUPRAMARK_MARKDOWN_OK) {
                std::string code;
                switch (status) {
                    case SUPRAMARK_MARKDOWN_ERR_SERIALIZE: code = "SERIALIZE_ERROR"; break;
                    case SUPRAMARK_MARKDOWN_ERR_NULL_INPUT: code = "NULL_INPUT"; break;
                    default: code = "UNKNOWN"; break;
                }
                if (outBuf) supramark_markdown_free(outBuf, outLen);
                promise.Reject(React::ReactError{
                    code.c_str(),
                    "supramark_markdown_parse_json failed"
                });
                return;
            }

            std::string json(outBuf, outLen);
            supramark_markdown_free(outBuf, outLen);
            promise.Resolve(std::move(json));
        }).detach();
    }

    REACT_METHOD(getVersion, L"getVersion")
    void getVersion(React::ReactPromise<std::string> promise) noexcept {
        const char *version = supramark_markdown_version();
        if (version) {
            promise.Resolve(std::string(version));
        } else {
            promise.Reject(React::ReactError{
                "UNKNOWN",
                "supramark_markdown_version returned NULL"
            });
        }
    }
};

} // namespace winrt::SupramarkMarkdownNative::implementation
