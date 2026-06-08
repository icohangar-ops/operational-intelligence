# Architecture

## Design principles

1. **Evidence before impression** — claims require `evidence_ids`; hiring findings cite transcript lines
2. **Specialist agents, shared state** — each agent has a narrow role; `WorkflowStore` persists context
3. **Tools as MCP descriptors** — `McpToolDescriptor` with JSON Schema inputs for capability negotiation
4. **Eval as quality gate** — `oi-eval` scores faithfulness before artifacts finalize
5. **Human-in-the-loop** — `ApprovalGate` blocks publication until reviewer approves

## Content crew pipeline

```
Research Agent
  └─ web_search tool → filter high-authority sources → Evidence[]
Analyst Agent
  └─ synthesize claims → structured Markdown outline
Writer Agent
  └─ outline → SEO technical article draft
Editor Agent
  └─ knowledge_base RAG → brand voice check → final article
Evaluator
  └─ faithfulness + hallucination scoring
Approval Gate (optional)
  └─ human review → completed workflow
```

## Hiring analysis pipeline

```
Parse transcript → TranscriptLine[]
Evidence Tracer → line-level Evidence for technical depth / uncertainty signals
Pattern Analyzer → hedging frequency, depth vs fluency patterns
Hiring Assessor → LLM synthesis constrained by rubric knowledge base
Evaluator → citation coverage scoring
Approval Gate → hiring decision review
```

## Operational intelligence

```
MockAnalyticsConnector
  └─ associative data model context (dimensions, measures, sample rows)
  └─ initiative → metric → ROI outcome mapping
  └─ natural-language query → evidence-backed business answer
```

## Observability

Every agent step emits a `TraceEvent` with:

- `phase`: Retrieve | Reason | Generate | Validate | Approve | Query
- `input_hash` / `output_hash` — SHA-256 of canonical JSON
- Optional HMAC signature via `OI_LEDGER_SIGNING_KEY`

Traces persist in `WorkflowStore` and are exposed at `GET /workflows/{id}/traces`.
