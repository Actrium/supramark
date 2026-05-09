//! Test-time LaTeX engine: drives the embedded MathJax bundle through an
//! in-process QuickJS runtime (rquickjs) so tests don't require an external
//! `node` binary.  Reused across e2e tests; see `latex::set_engine` for the
//! injection point.

use std::cell::RefCell;
use std::sync::Once;

use rquickjs::{Context, Function, Object, Runtime};

use d2_little::latex;

static INSTALLED: Once = Once::new();

thread_local! {
    static MATHJAX_RUNTIME: RefCell<Option<MathJaxRuntime>> = const { RefCell::new(None) };
}

struct MathJaxRuntime {
    _runtime: Runtime,
    context: Context,
}

impl MathJaxRuntime {
    fn new() -> Result<Self, String> {
        let runtime = Runtime::new().map_err(|e| format!("create rquickjs runtime: {e:?}"))?;
        // 0 disables QuickJS' default memory cap; MathJax allocates a fair
        // amount during init and a smaller cap aborts large formulae.
        runtime.set_memory_limit(0);

        let context =
            Context::full(&runtime).map_err(|e| format!("create rquickjs context: {e:?}"))?;

        let init: Result<(), String> = context.with(|ctx| {
            // Shim `console` — MathJax may call console.warn for unknown
            // macros / packages.  Silent no-ops keep stderr clean and avoid
            // ReferenceErrors during eval.
            let console = Object::new(ctx.clone()).map_err(|e| format!("alloc console: {e:?}"))?;
            for name in ["log", "warn", "error", "info", "debug"] {
                let f = Function::new(ctx.clone(), || {})
                    .map_err(|e| format!("alloc console.{name}: {e:?}"))?;
                console
                    .set(name, f)
                    .map_err(|e| format!("set console.{name}: {e:?}"))?;
            }
            ctx.globals()
                .set("console", console)
                .map_err(|e| format!("install console: {e:?}"))?;

            ctx.eval::<(), _>(latex::MATHJAX_JS)
                .map_err(|e| format!("eval mathjax.js: {e:?}"))?;
            ctx.eval::<(), _>(latex::SETUP_JS)
                .map_err(|e| format!("eval setup.js: {e:?}"))?;
            Ok(())
        });
        init?;

        Ok(Self {
            _runtime: runtime,
            context,
        })
    }

    fn convert(&self, latex_src: &str) -> Result<String, String> {
        // Reset the TeX inputJax's label registry between calls.  MathDocument
        // keeps `parseOptions.tags.{labels, allLabels, allIds}` on the
        // inputJax — without resetting them, a second pass over the same
        // `\label{...}` collides and MathJax produces a degraded SVG (e.g.
        // single-line where the formula is multi-line).  `inputJax.reset()`
        // forwards to `parseOptions.tags.reset(0)` which empties the
        // registry; `MathDocument.reset()` alone does not cascade here.
        let expr = format!(
            "for (const ij of html.inputJax) ij.reset(); {}",
            latex::build_convert_expr(latex_src)
        );
        self.context.with(|ctx| {
            ctx.eval::<String, _>(expr.as_str())
                .map_err(|e| format!("eval MathJax convert: {e:?}"))
        })
    }
}

fn render_via_quickjs(latex_src: &str) -> Result<String, String> {
    MATHJAX_RUNTIME.with(|cell| {
        let mut slot = cell.borrow_mut();
        if slot.is_none() {
            *slot = Some(MathJaxRuntime::new()?);
        }
        slot.as_ref().unwrap().convert(latex_src)
    })
}

/// Install the rquickjs-backed engine.  Idempotent across calls and across
/// test files; safe to call from each test entry point.
pub fn install() {
    INSTALLED.call_once(|| {
        latex::set_engine(render_via_quickjs);
    });
}
