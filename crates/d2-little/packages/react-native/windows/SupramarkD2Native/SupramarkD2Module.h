/*
 * SupramarkD2Module.h (Windows)
 *
 * C++/WinRT React Native module for D2 diagram rendering on Windows.
 * Bridges JS render(source) calls to the C ABI exported by
 * supramark_d2_native.dll:
 *
 *   int32_t supramark_d2_render(const char* input, size_t input_len,
 *                               char** out_buf, size_t* out_len);
 *   void    supramark_d2_free(char* buf, size_t len);
 *   const char* supramark_d2_version(void);
 *
 * SPDX-License-Identifier: MPL-2.0
 */

#pragma once

#include <string>
#include <thread>

#include <NativeModules.h>

extern "C" {
#include "supramark_d2.h"
}

namespace winrt::SupramarkD2Native::implementation {

REACT_MODULE(SupramarkD2Module, L"SupramarkD2Native")
struct SupramarkD2Module {

    REACT_METHOD(render, L"render")
    void render(std::string source, React::ReactPromise<std::string> promise) noexcept {
        // Dispatch to a worker thread to avoid blocking the JS thread.
        std::thread([source = std::move(source), promise = std::move(promise)]() mutable {
            char *outBuf = nullptr;
            size_t outLen = 0;

            int32_t status = supramark_d2_render(
                source.c_str(), source.size(), &outBuf, &outLen);

            if (status != SUPRAMARK_D2_OK) {
                std::string code;
                switch (status) {
                    case SUPRAMARK_D2_ERR_PARSE:     code = "PARSE_ERROR"; break;
                    case SUPRAMARK_D2_ERR_RENDER:    code = "RENDER_ERROR"; break;
                    case SUPRAMARK_D2_ERR_NULL_INPUT: code = "NULL_INPUT"; break;
                    default:                          code = "UNKNOWN"; break;
                }
                if (outBuf) supramark_d2_free(outBuf, outLen);
                promise.Reject(React::ReactError{
                    code.c_str(),
                    "supramark_d2_render failed"
                });
                return;
            }

            std::string svg(outBuf, outLen);
            supramark_d2_free(outBuf, outLen);
            promise.Resolve(std::move(svg));
        }).detach();
    }

    REACT_METHOD(getVersion, L"getVersion")
    void getVersion(React::ReactPromise<std::string> promise) noexcept {
        const char *version = supramark_d2_version();
        if (version) {
            promise.Resolve(std::string(version));
        } else {
            promise.Reject(React::ReactError{
                "UNKNOWN",
                "supramark_d2_version returned NULL"
            });
        }
    }
};

} // namespace winrt::SupramarkD2Native::implementation
