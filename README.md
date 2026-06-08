# Operational Intelligence Platform

Rust-native multi-agent platform that maps live business data to strategic initiatives and ROI outcomes — with evidence-traced reasoning, MCP-style tools, and human-in-the-loop approval.

Built by [Cubiczan](https://codeberg.org/cubiczan). Composes patterns from enterprise OI (Lemmata-style), agentic content crews, and evidence-first hiring transcript analysis.

## Three workflow modes

| Mode | Agents | Output |
|------|--------|--------|
| **Content Crew** | Research → Analyst → Writer → Editor | SEO technical article + outline |
| **Hiring Analysis** | Evidence Tracer → Pattern Analyzer → Assessor | Line-cited hiring assessment |
| **Operational Query** | Data Connector → Reasoning → ROI mapping | Initiative-linked business insight |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                        oi-api / oi-cli                       │
└────────────┬──────────────────────┬─────────────────────────┘
             │                      │
    ┌────────▼────────┐    ┌────────▼────────┐    ┌──────────┐
    │    oi-crew      │    │   oi-hiring     │    │oi-connector│
    │ 4-agent content │    │ transcript OI   │    │ data/ROI  │
    └────────┬────────┘    └────────┬────────┘    └────┬─────┘
             │                      │                     │
    ┌────────▼──────────────────────▼─────────────────────▼────┐
    │  oi-tools (MCP)  ·  oi-llm  ·  oi-eval  ·  oi-memory    │
    └──────────────────────────┬─────────────────────────────────┘
                               │
                         oi-core (evidence, audit, workflows)
```

## Quick start

```bash
cargo build --release -p oi-cli

# Content crew (runs offline with mock LLM + web search)
./target/release/oi crew "operational intelligence trends 2026"

# Hiring transcript analysis
./target/release/oi hiring --file examples/sample-transcript.txt

# Operational intelligence query (mock analytics connector)
./target/release/oi query "What is our pipeline coverage vs Q3 target?"

# REST API
./target/release/oi serve
```

## API endpoints

| Method | Path | Description |
|--------|------|-------------|
| `GET` | `/health` | Liveness |
| `POST` | `/workflows/crew` | Start content crew `{ "topic": "..." }` |
| `POST` | `/workflows/hiring` | Analyze transcript `{ "transcript": "..." }` |
| `POST` | `/workflows/{id}/approve` | Human-in-the-loop approval |
| `GET` | `/workflows/{id}/traces` | OpenTelemetry-style audit traces |
| `POST` | `/oi/query` | Operational intelligence query |
| `GET` | `/oi/initiatives` | List strategic initiatives |

## Key capabilities

- **Evidence-traced claims** — every conclusion links to source evidence with authority scoring
- **MCP-style tools** — `web_search`, `knowledge_base`, `read_file` with JSON schemas
- **Faithfulness eval** — hallucination risk scoring before publication
- **HITL approval gates** — workflows pause for human review before finalization
- **Signed audit traces** — full traceability from business question to validated answer
- **Mock-first** — runs fully offline; swap `MockLlm` for HTTP OpenAI-compatible backend

## Crates

| Crate | Purpose |
|-------|---------|
| `oi-core` | Evidence, initiatives, ROI, workflow state, audit |
| `oi-llm` | LLM provider trait (mock + HTTP) |
| `oi-tools` | MCP-style tool registry |
| `oi-memory` | Persistent workflow + trace store |
| `oi-eval` | Faithfulness / hallucination evaluation |
| `oi-crew` | Research / Analyst / Writer / Editor orchestration |
| `oi-hiring` | Transcript analysis with line-level evidence |
| `oi-connector` | Analytics data model connector (mock Qlik-style) |
| `oi-api` | Axum REST API |
| `oi-cli` | `oi` command-line interface |

## License

MIT
