#!/usr/bin/env bash
# Generate reference SVG files from the original Java PlantUML v1.2026.2.
#
# Prerequisites:
#   - java on PATH (JDK 17+)
#   - plantuml-1.2026.2.jar (auto-resolved; see PLANTUML_JAR below)
#   - Graphviz available to Java:
#       * by default: the `dot` binary on PATH
#       * OR (when PLANTUML_LITTLE_REF_BACKEND=wasm) the shared
#         wasm Graphviz wrapper at scripts/wasm-dot-wrapper.sh, which
#         requires `node` on PATH and `npm install` having been run
#         in tests/support/
#
# Environment variables:
#   PLANTUML_JAR                Path to plantuml jar. If unset, tries:
#                                 1. $1 (first arg)
#                                 2. /ext/plantuml/plantuml/build/libs/plantuml-*.jar
#                                 3. /tmp/plantuml-cache/plantuml-1.2026.2.jar
#                                 4. tests/tools/plantuml-1.2026.2.jar
#   PLANTUML_LITTLE_REF_BACKEND When set to `wasm`, routes Graphviz calls
#                                 through scripts/wasm-dot-wrapper.sh.
#                                 See scripts/wasm-dot-wrapper.sh for why.
#
# Usage:
#   bash tests/generate_reference.sh [plantuml.jar]
#   PLANTUML_LITTLE_REF_BACKEND=wasm bash tests/generate_reference.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FIXTURES_DIR="$SCRIPT_DIR/fixtures"
GOLDEN_DIR="$SCRIPT_DIR/reference"

# --- Resolve plantuml.jar ---------------------------------------------------

if [[ $# -ge 1 ]]; then
    PLANTUML_JAR="$1"
elif [[ -n "${PLANTUML_JAR:-}" ]]; then
    : # use env
else
    # Probe common locations in order of preference.
    candidates=(
        "$(find /ext/plantuml/plantuml/build/libs -name 'plantuml-*.jar' \
            ! -name '*-sources*' ! -name '*-javadoc*' 2>/dev/null | head -1)"
        "/tmp/plantuml-cache/plantuml-1.2026.2.jar"
        "${PROJECT_DIR}/tests/tools/plantuml-1.2026.2.jar"
    )
    PLANTUML_JAR=""
    for c in "${candidates[@]}"; do
        if [[ -n "$c" && -f "$c" ]]; then
            PLANTUML_JAR="$c"
            break
        fi
    done
    if [[ -z "$PLANTUML_JAR" ]]; then
        echo "ERROR: plantuml.jar not found. Either build it, pass it as" >&2
        echo "arg, set \$PLANTUML_JAR, or stage it at one of:" >&2
        printf '  %s\n' "${candidates[@]}" >&2
        echo "" >&2
        echo "Quick cache:" >&2
        echo "  mkdir -p /tmp/plantuml-cache && curl -fL -o \\" >&2
        echo "    /tmp/plantuml-cache/plantuml-1.2026.2.jar \\" >&2
        echo "    https://github.com/plantuml/plantuml/releases/download/v1.2026.2/plantuml-1.2026.2.jar" >&2
        exit 1
    fi
fi

# --- Optionally route dot calls through the shared wasm wrapper -------------

DOT_WRAPPER="${PROJECT_DIR}/scripts/wasm-dot-wrapper.sh"
if [[ "${PLANTUML_LITTLE_REF_BACKEND:-}" == "wasm" ]]; then
    if [[ ! -x "$DOT_WRAPPER" ]]; then
        echo "ERROR: PLANTUML_LITTLE_REF_BACKEND=wasm but $DOT_WRAPPER is missing or not executable." >&2
        exit 1
    fi
    if [[ ! -d "${PROJECT_DIR}/tests/support/node_modules/@actrium/graphviz-anywhere-web" ]]; then
        echo "ERROR: tests/support/node_modules missing. Run:" >&2
        echo "  (cd ${PROJECT_DIR}/tests/support && npm install)" >&2
        exit 1
    fi
    export GRAPHVIZ_DOT="$DOT_WRAPPER"
    DOT_VERSION_LINE="$("$DOT_WRAPPER" -V 2>&1 | head -1)"
    BACKEND_LABEL="wasm (scripts/wasm-dot-wrapper.sh)"
else
    DOT_VERSION_LINE="$(dot -V 2>&1 | head -1 || echo "unknown")"
    BACKEND_LABEL="native (\$GRAPHVIZ_DOT or PATH's dot)"
fi

echo "Using plantuml.jar: $PLANTUML_JAR"
echo "Graphviz backend:   $BACKEND_LABEL"
echo "Fixtures dir:       $FIXTURES_DIR"
echo "Reference dir:      $GOLDEN_DIR"

# --- Record environment snapshot into tests/reference/VERSION ---------------

mkdir -p "$GOLDEN_DIR"
cat > "$GOLDEN_DIR/VERSION" <<EOF
plantuml_jar: $PLANTUML_JAR
plantuml_git: $(cd /ext/plantuml/plantuml 2>/dev/null && git rev-parse HEAD 2>/dev/null || echo "unknown")
java_version: $(java -version 2>&1 | head -1)
graphviz_backend: $BACKEND_LABEL
dot_version: $DOT_VERSION_LINE
generated_at: $(date -u +%Y-%m-%dT%H:%M:%SZ)
EOF

echo "---"
cat "$GOLDEN_DIR/VERSION"
echo "---"

# --- Generate reference SVGs -----------------------------------------------

total=0
success=0
failed=0
skipped=0

find "$FIXTURES_DIR" -name '*.puml' -type f | sort | while read -r puml; do
    # Compute relative path: fixtures/class/foo.puml -> class/foo
    rel="${puml#$FIXTURES_DIR/}"
    category="$(dirname "$rel")"
    basename="$(basename "$rel" .puml)"

    out_dir="$GOLDEN_DIR/$category"
    mkdir -p "$out_dir"

    out_svg="$out_dir/$basename.svg"

    total=$((total + 1))

    if java -jar "$PLANTUML_JAR" -tsvg -pipe < "$puml" > "$out_svg" 2>/dev/null; then
        if grep -q '<svg' "$out_svg"; then
            success=$((success + 1))
        else
            echo "WARN: $rel - no <svg> in output, removing"
            rm -f "$out_svg"
            skipped=$((skipped + 1))
        fi
    else
        echo "FAIL: $rel"
        rm -f "$out_svg"
        failed=$((failed + 1))
    fi
done

echo ""
echo "Done. Generated reference SVGs in: $GOLDEN_DIR"
echo "Check $GOLDEN_DIR/VERSION for environment details."
