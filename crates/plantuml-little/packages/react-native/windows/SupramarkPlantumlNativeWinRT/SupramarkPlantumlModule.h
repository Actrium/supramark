/*
 * SupramarkPlantumlModule.h (Windows)
 *
 * C++/WinRT React Native module for PlantUML rendering on Windows.
 * Bridges JS render(source) calls to the C ABI exported by
 * supramark_plantuml_native.dll:
 *
 *   int  supramark_plantuml_render(const char *input, size_t input_len,
 *                                  uint8_t **out_buf, size_t *out_len);
 *   void supramark_plantuml_free(uint8_t *buf, size_t len);
 *   const char *supramark_plantuml_version(void);
 *
 * SPDX-License-Identifier: GPL-3.0-or-later OR LGPL-3.0-or-later OR Apache-2.0 OR EPL-2.0 OR MIT
 */

#pragma once

#include <string>
#include <thread>
#include <cstdint>

#include <NativeModules.h>

extern "C" {
#include "supramark_plantuml.h"
}

namespace winrt::SupramarkPlantumlNative::implementation {

REACT_MODULE(SupramarkPlantumlModule, L"SupramarkPlantumlNative")
struct SupramarkPlantumlModule {

    REACT_METHOD(render, L"render")
    void render(std::string source, React::ReactPromise<std::string> promise) noexcept {
        // Dispatch to a worker thread to avoid blocking the JS thread.
        std::thread([source = std::move(source), promise = std::move(promise)]() mutable {
            uint8_t *outBuf = nullptr;
            size_t outLen = 0;

            int status = supramark_plantuml_render(
                source.c_str(), source.size(), &outBuf, &outLen);

            if (status != SUPRAMARK_PLANTUML_OK) {
                std::string code;
                switch (status) {
                    case SUPRAMARK_PLANTUML_ERR_PARSE:     code = "PARSE_ERROR"; break;
                    case SUPRAMARK_PLANTUML_ERR_RENDER:    code = "RENDER_ERROR"; break;
                    case SUPRAMARK_PLANTUML_ERR_NULL_INPUT: code = "NULL_INPUT"; break;
                    default:                                code = "UNKNOWN"; break;
                }
                if (outBuf) supramark_plantuml_free(outBuf, outLen);
                promise.Reject(React::ReactError{
                    code.c_str(),
                    "supramark_plantuml_render failed"
                });
                return;
            }

            std::string svg(reinterpret_cast<const char *>(outBuf), outLen);
            supramark_plantuml_free(outBuf, outLen);
            promise.Resolve(std::move(svg));
        }).detach();
    }

    REACT_METHOD(getVersion, L"getVersion")
    void getVersion(React::ReactPromise<std::string> promise) noexcept {
        const char *version = supramark_plantuml_version();
        if (version) {
            promise.Resolve(std::string(version));
        } else {
            promise.Reject(React::ReactError{
                "UNKNOWN",
                "supramark_plantuml_version returned NULL"
            });
        }
    }
};

} // namespace winrt::SupramarkPlantumlNative::implementation
