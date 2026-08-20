/*
 * SupramarkMermaidModule.h (Windows)
 *
 * C++/WinRT React Native module for Mermaid rendering on Windows.
 * Bridges JS render(source) calls to the C ABI exported by
 * supramark_mermaid_native.dll:
 *
 *   int32_t supramark_mermaid_render(const uint8_t *input, size_t input_len,
 *                                    uint8_t **out_buf, size_t *out_len);
 *   void    supramark_mermaid_free(uint8_t *buf, size_t len);
 *   const char *supramark_mermaid_version(void);
 *
 * Licensed under the Apache License, Version 2.0
 */

#pragma once

#include <string>
#include <thread>
#include <cstdint>

#include <NativeModules.h>

extern "C" {
#include "supramark_mermaid.h"
}

namespace winrt::SupramarkMermaidNative::implementation {

REACT_MODULE(SupramarkMermaidModule, L"SupramarkMermaidNative")
struct SupramarkMermaidModule {

    REACT_METHOD(render, L"render")
    void render(std::string source, React::ReactPromise<std::string> promise) noexcept {
        // Dispatch to a worker thread to avoid blocking the JS thread.
        std::thread([source = std::move(source), promise = std::move(promise)]() mutable {
            const uint8_t *in = reinterpret_cast<const uint8_t *>(source.data());
            uint8_t *outBuf = nullptr;
            size_t outLen = 0;

            int32_t status = supramark_mermaid_render(in, source.size(), &outBuf, &outLen);

            if (status != SUPRAMARK_MERMAID_OK) {
                std::string code;
                switch (status) {
                    case SUPRAMARK_MERMAID_ERR_PARSE:     code = "PARSE_ERROR"; break;
                    case SUPRAMARK_MERMAID_ERR_RENDER:    code = "RENDER_ERROR"; break;
                    case SUPRAMARK_MERMAID_ERR_NULL_INPUT: code = "NULL_INPUT"; break;
                    default:                               code = "UNKNOWN"; break;
                }
                if (outBuf) supramark_mermaid_free(outBuf, outLen);
                promise.Reject(React::ReactError{
                    code.c_str(),
                    "supramark_mermaid_render failed"
                });
                return;
            }

            std::string svg(reinterpret_cast<const char *>(outBuf), outLen);
            supramark_mermaid_free(outBuf, outLen);
            promise.Resolve(std::move(svg));
        }).detach();
    }

    REACT_METHOD(getVersion, L"getVersion")
    void getVersion(React::ReactPromise<std::string> promise) noexcept {
        const char *version = supramark_mermaid_version();
        if (version) {
            promise.Resolve(std::string(version));
        } else {
            promise.Reject(React::ReactError{
                "UNKNOWN",
                "supramark_mermaid_version returned NULL"
            });
        }
    }
};

} // namespace winrt::SupramarkMermaidNative::implementation
