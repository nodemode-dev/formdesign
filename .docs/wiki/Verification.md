# Verification

What every change is checked against before it lands. CI does the mechanical part; the rest is done by the person (or agent) making the change, and the result is written in the pull request: verified with throttling, verified without and why, or not verified.

## What CI runs

`.github/workflows/ci.yml` on every push to `main` and every pull request:

| Job | Checks |
| --- | --- |
| `frontend` | `npm ci`, then `npm run build` (type check and Vite build) |
| `backend` | install, `compileall`, `formdes_engine.py --selfcheck`, the template linter over the whole library, `agents sync --check`, booting the engine and probing `/api/health`, `/api/capabilities`, `/api/templates` |
| `layout` | builds the core, then lays out a newsletter (PDF and `.fdes`) and a flyer (PNG) from the command line |
| `scripts` | `bash -n` over every shell script, and a scan of tracked text for em dashes and emoji |

## The Make sweep

Before a release, every template is made twice and every report must come back without an error:

```bash
for id in $(./formdes templates --json | python3 -c "import json,sys; print(' '.join(t['id'] for t in json.load(sys.stdin)))"); do
  ./formdes make --template $id --brief "A feature for Harbour magazine, Spring issue. CTA: Read more" article.md photo1.jpg photo2.jpg --out /tmp/sweep/$id.fdes
  ./formdes make --template $id --brief "a poster for the Harbour Jazz Festival, Friday 12 June 2026, 7pm to 11pm" --out /tmp/sweep/$id-brief.fdes
done
```

Look at a few renders each time (`--out x.png`), especially templates the change touched.

## The throttled UI check

Every change that touches the window:

1. Open Chrome DevTools, Performance, set CPU to 4x slowdown and the network to a slow profile; disable the cache.
2. Hard-reload with a document open. There must be no frame in the wrong theme, no layout jump, no blank canvas where tiles were.
3. Exercise every path the change touched, and watch the moment of change, not the end state: panels patch values without a section disappearing and coming back, lists keep their scroll and focus, the canvas keeps its old tiles until new ones arrive.
4. Change the selection a few times with a panel open. A mutation observer on the panel column should see text nodes and option lists change, never whole sections rebuilt.

At 0.6.1 the main panels passed steps 2 to 4; the rulers, Glyphs, Tabs, Table, Book and Story Editor added at 0.7.0 have been exercised but not yet throttled.

## Before calling a template done

`validate_template` (or the checker in [[Templates]]) reports no problems, no overset outside flows and no preflight errors, and every rendered page has been looked at.

## The desktop smoke test

After `./formdes desktop build`, launch the app with port 8196 already taken: it must still open on another port, show the home screen, and leave no engine or core process behind when it quits.

## Tests

formdes has no self-written test suite by design: tests written in the same session as the code tend to encode the code's own assumptions. The behaviours to test are specified in `TEST_PLAN.md` in the planning sidecar, written for someone who has not seen the implementation.
