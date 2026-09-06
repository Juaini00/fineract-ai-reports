# Bundle B3 — W-A1 Analyst Question Inventory (Implementation Plan)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Author `docs/product/analyst-question-inventory.md` — a bilingual (Indonesian + English) catalogue of ≥25 real analyst questions across the savings, client, and organization domains, each mapped to a capability id and a `covered` / `partial` / `missing` verdict against the **current** knowledge catalog. Loan questions are enumerated but marked `missing` and pointed at issue 008 (loans are split out of 007). This is a documentation deliverable only: **no Rust, no YAML, no SQL, no migrations change.**

**Architecture:** One new markdown file, built up over four tasks. Task 1 lands a concrete STARTER draft (≥10 questions) so the seed is real; Tasks 2–3 extend it to full domain coverage; Task 4 is the machine-checkable validation pass (question count, capability-id existence, verdict legend). Every capability id cited in the doc must resolve to a real file under `knowledge/capabilities/**`. No behavior changes anywhere in the workspace.

**Tech Stack:** Markdown only. Verification uses `grep`/`awk`/`ls` against the existing repo tree — no build, no test harness, no new dependency.

**Authoritative sources (read-only inputs):**
- `knowledge/capabilities/**/*.yaml` — the 30 `approved_mvp` capabilities and their `examples:` blocks.
- `docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md` — Problem section representative question (lines 20–30), W-A (lines 186–253), Appendix A domain facts (lines 1549+).
- `docs/issues/active/008-loan-domain-analyst-capabilities.md` — the five loan capability ids in priority order (lines 22–28).
- `docs/reporting-data/*.md` — field-level domain facts.

## Current state (verified 2026-07-27)

Audited before writing this plan; the issue text (dated 2026-07-24) is stale in two places the inventory depends on:

- **The catalog has 30 `approved_mvp` capabilities, zero loan capabilities.** Domains: `savings` (11), `client` (10), `organization` (9). Loans are entirely absent — every loan question is therefore `missing`, and issue 008 owns them.
- **`savings_pending_charges_clients` is already enriched (issue E3 is STALE).** Its `output_fields` today are `office_id, office_name, savings_account_id, savings_account_charge_id, charge_definition_id, charge_name, is_penalty, charge_timing_enum, currency_code, amount_due_current, amount_paid, amount_waived, amount_written_off, amount_outstanding, due_date, days_overdue` plus PII-conditional `client_id, client_display_name`. The "only 9 shallow columns … omits `days_overdue`, `amount_paid`, `amount_waived`" claim in E3/§Problem no longer holds. **Consequence:** the representative analyst question (007 §Problem, lines 20–30) is now `covered` by exactly one capability — the inventory records it as such, not as a gap.
- **One honest residual on that capability:** it exposes `amount_due_current` (current/next occurrence) but **not** `amount_levied_total` (total ever levied across occurrences), which A.1.3 Finding 1 recommends and the program roadmap folds into Bundle 4. The inventory marks the "how much was this charge in total over its life?" phrasing `partial` so the gap stays visible for W-A3.
- **The only genuinely user-required parameter in the whole catalog is the search term on `client_name_lookup`.** Every other parameter (`office_ids`, dates, `limit`) has a policy default (`authorized_scope`, `business_today`, `unbounded`/`hard_cap`), so no other question forces a required input from the user. The inventory's "user-required param?" column is `name` for the lookup and `none` everywhere else.

## Global Constraints

- **Documentation only.** Do not touch any `.rs`, `.yaml`, `.sql`, or `migrations/**` file. The workspace must build identically before and after (`cargo check` is not even required — nothing compilable changes).
- **English-only product copy in prose;** the Indonesian column holds only the analyst's own phrasing (a data value), matching the bilingual `examples:` already present in the YAML catalog.
- Every capability id written into the doc must be an id that exists in `knowledge/capabilities/**` (savings/client/org) **or** one of the five loan ids reserved in issue 008. No invented ids.
- Loan rows are `missing` and cross-reference issue 008; do **not** invent loan field sets beyond the capability names 008 reserves.
- Verdict legend is fixed: `covered` = one existing capability returns the full required field set with sane defaults; `partial` = a capability answers the intent but a listed field or variant is absent; `missing` = no approved capability answers it.
- **No commit steps.** A task is done when its listed checks exit `0`. The user commits manually.

---

## Task 1: Land the file skeleton and a real STARTER draft (≥10 questions)

Create the doc with its header, method, legend, and a first savings-heavy block of real rows so execution has a concrete seed to extend. Every row here is final content, not a placeholder.

**Files:**
- Create: `docs/product/analyst-question-inventory.md`

- [ ] **Step 1: Confirm the doc does not already exist and the cited savings/client capability ids are real**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
test ! -e docs/product/analyst-question-inventory.md && echo "absent: ok to create"
for id in savings_balance_summary savings_pending_charges_clients savings_deposit_total \
          savings_withdrawal_total savings_deposit_top_n savings_withdrawal_top_n \
          savings_activity_list client_list_recent client_name_lookup \
          client_lifecycle_summary; do
  grep -rl "^id: $id$" knowledge/capabilities >/dev/null && echo "ok  $id" || echo "MISSING $id"
done
```
Expected: prints `absent: ok to create` and `ok  <id>` for all ten ids (no `MISSING`). If any prints `MISSING`, stop and re-audit the catalog before writing that row.

- [ ] **Step 2: Write the skeleton and starter rows**

Create `docs/product/analyst-question-inventory.md` with exactly this content:

````markdown
# Analyst Question Inventory

**Purpose:** Prove the knowledge catalog answers the questions a banking analyst actually asks. Each entry pairs the real Indonesian and English phrasing with the field set the answer must contain, whether any parameter is genuinely required from the user, the capability that answers it, and a coverage verdict against the current catalog.

**Scope:** Savings, client, and organization domains (issue 007). Loan questions are enumerated here for visibility but are owned by issue 008 — they are all `missing` in 007 until 008 ships.

**Verified:** 2026-07-27 against `knowledge/capabilities/**` (30 `approved_mvp` capabilities: 11 savings, 10 client, 9 organization; 0 loan).

## Legend

| Verdict | Meaning |
| --- | --- |
| `covered` | One existing capability returns the full required field set with sane defaults; no clarification needed. |
| `partial` | A capability answers the intent, but a listed field or variant is absent. Feeds W-A3. |
| `missing` | No approved capability answers it. |

**User-required param:** `none` means every parameter has a policy default (`authorized_scope` for offices, `business_today` for dates, `unbounded`/`hard_cap` for limit), so the system fills silently and proceeds. The only genuine user-required input in the current catalog is the search term on `client_name_lookup`.

## Savings domain

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Berapa saldo total tabungan aktif saat ini? | What is the total active savings balance right now? | active account count, total balance, currency | none | `savings_balance_summary` | covered |
| 2 | Beritahukan saya siapa saja yang masih memiliki charge yang belum dibayar hari ini pada savings, sebutkan jenis charge-nya, tanggal due-nya atau sudah lewat berapa hari, yang dibayar berapa dan sisa berapa | Which clients still have unpaid savings charges today — with charge type, due date, days overdue, amount paid, and amount outstanding? | client identity, charge name, is_penalty, charge timing, due date, days overdue, amount due current, amount paid, amount waived, amount written off, amount outstanding, currency | none | `savings_pending_charges_clients` | covered |
| 3 | Berapa total penarikan bulan ini? | What is the total withdrawal this month? | period, total withdrawal amount, currency | none | `savings_withdrawal_total` | covered |
| 4 | Berapa total setoran bulan ini? | What is the total deposit this month? | period, total deposit amount, currency | none | `savings_deposit_total` | covered |
| 5 | Penarikan terbesar bulan ini siapa? | Who made the largest withdrawals this month? | client identity, amount, date, rank | none | `savings_withdrawal_top_n` | covered |
| 6 | Setoran terbesar hari ini siapa? | Who made the largest deposits today? | client identity, amount, date, rank | none | `savings_deposit_top_n` | covered |
| 7 | Tunjukkan aktivitas tabungan minggu ini | Show savings activity this week | account, transaction type, amount, date | none | `savings_activity_list` | covered |
| 8 | Berapa setoran tabungan per bulan tahun ini? | Monthly deposit totals for this year | month, total deposit amount, currency | none | `savings_deposit_monthly_breakdown` | covered |
| 9 | Berapa penarikan tabungan per bulan tahun ini? | Monthly withdrawal totals for this year | month, total withdrawal amount, currency | none | `savings_withdrawal_monthly_breakdown` | covered |
| 10 | Setoran terbesar setiap bulan tahun ini | Largest deposit for each month this year | month, client identity, amount | none | `savings_deposit_monthly_top_n` | covered |

## Client domain

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| 11 | Tunjukkan nasabah yang baru diaktivasi | Show the most recently activated clients | client identity, activation date, office | none | `client_list_recent` | covered |
| 12 | Ada gak nama Tony di client kita? | Is there a client named Tony? | client identity, office, status | name | `client_name_lookup` | covered |
````

- [ ] **Step 3: Validate the starter draft**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
grep -cE '^\| [0-9]+ \|' docs/product/analyst-question-inventory.md
```
Expected: prints `12` (twelve numbered question rows seeded). If lower, a row was dropped — re-add it.

---

## Task 2: Extend to full client + organization coverage

Append the remaining client rows and the full organization block. Continue the `#` numbering from 13.

**Files:**
- Modify: `docs/product/analyst-question-inventory.md`

- [ ] **Step 1: Confirm the remaining client + organization capability ids are real**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
for id in client_activation_monthly_breakdown client_activation_top_n_offices \
          client_random_sample client_summary_by_office client_top_n_by_deposit_volume \
          client_top_n_by_savings_account_count client_top_n_by_savings_balance \
          organization_hierarchy_summary organization_office_activity_ranking \
          organization_office_client_summary organization_office_dormant \
          organization_office_hierarchy_tree office_list_basic \
          organization_office_opening_monthly_breakdown organization_office_savings_summary \
          organization_office_summary; do
  grep -rl "^id: $id$" knowledge/capabilities >/dev/null && echo "ok  $id" || echo "MISSING $id"
done
```
Expected: `ok  <id>` for all sixteen (no `MISSING`).

- [ ] **Step 2: Append the remaining client rows**

Append to `docs/product/analyst-question-inventory.md` (immediately after row 12, inside the Client domain table):
```markdown
| 13 | Berapa banyak nasabah diaktivasi tiap bulan tahun lalu? | How many clients did we activate each month last year? | month, activation count | none | `client_activation_monthly_breakdown` | covered |
| 14 | Kantor mana yang paling banyak mengaktivasi nasabah bulan ini? | Top offices by new client activations this month | office, activation count, rank | none | `client_activation_top_n_offices` | covered |
| 15 | Coba berikan saya 5 client sembarang pada tahun ini | Give me a random sample of 5 clients | client identity, office | none | `client_random_sample` | covered |
| 16 | Tampilkan ringkasan lifecycle nasabah (pending, aktif, tutup) | Show the client lifecycle summary (pending, active, closed counts) | status, count | none | `client_lifecycle_summary` | covered |
| 17 | Berapa jumlah nasabah aktif per kantor? | How many active clients does each office have? | office, lifecycle counts | none | `client_summary_by_office` | covered |
| 18 | Nasabah mana yang paling banyak menyetor bulan ini? | Top clients by deposit volume this month | client identity, deposit volume, rank, currency | none | `client_top_n_by_deposit_volume` | covered |
| 19 | Nasabah mana yang punya rekening tabungan terbanyak? | Which clients have the most savings accounts? | client identity, account count, rank | none | `client_top_n_by_savings_account_count` | covered |
| 20 | Nasabah mana dengan saldo tabungan tertinggi? | Which clients hold the highest savings balances? | client identity, balance, rank, currency | none | `client_top_n_by_savings_balance` | covered |
```

- [ ] **Step 3: Append the organization block**

Append to `docs/product/analyst-question-inventory.md`:
```markdown
## Organization domain

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| 21 | Tunjukkan ringkasan hierarki kantor | Show the office hierarchy summary | root count, leaf count, max depth | none | `organization_hierarchy_summary` | covered |
| 22 | Kantor mana yang paling banyak transaksinya bulan ini? | Which offices had the most transactions this month? | office, transaction count, rank | none | `organization_office_activity_ranking` | covered |
| 23 | Tampilkan jumlah nasabah per kantor | Show client counts per office | office, client count | none | `organization_office_client_summary` | covered |
| 24 | Kantor mana yang tidak ada aktivitas kuartal ini? | Which offices had no activity this quarter? | office, last activity date | none | `organization_office_dormant` | covered |
| 25 | Tampilkan pohon hierarki kantor saya | Show my office hierarchy tree | office, parent, depth | none | `organization_office_hierarchy_tree` | covered |
| 26 | Berikan 3 office yang ada pada system saat ini | List the offices in my authorized scope | office id, office name | none | `office_list_basic` | covered |
| 27 | Berapa kantor dibuka tiap bulan tahun ini? | How many offices opened each month this year? | month, office opening count | none | `organization_office_opening_monthly_breakdown` | covered |
| 28 | Kantor mana yang memegang saldo tabungan terbesar? | Which office holds the most savings? | office, savings balance, rank, currency | none | `organization_office_savings_summary` | covered |
| 29 | Tampilkan ringkasan kantor beserta jumlah staf aktif | Show the office summary with active staff count | office, active staff count, client count | none | `organization_office_summary` | covered |
```

- [ ] **Step 4: Validate the count crossed 25**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
grep -cE '^\| [0-9]+ \|' docs/product/analyst-question-inventory.md
```
Expected: prints `29`.

---

## Task 3: Append the gap rows (savings partials) and the loan section (all `missing` → 008)

Record the honest gaps: the savings partials W-A3 must close, and every loan question — enumerated for visibility, all `missing`, pointed at issue 008.

**Files:**
- Modify: `docs/product/analyst-question-inventory.md`

- [ ] **Step 1: Confirm the loan ids match the ones issue 008 reserves**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
grep -nE '^[0-9]+\. `loan' docs/issues/active/008-loan-domain-analyst-capabilities.md
grep -rl '^id: loan' knowledge/capabilities >/dev/null 2>&1 && echo "UNEXPECTED loan capability exists" || echo "ok: no loan capability in catalog"
```
Expected: the first lists the five ids `loans_in_arrears_clients`, `loan_overdue_installments`, `loan_outstanding_balances_clients`, `loan_unpaid_charges_clients`, `loan_portfolio_summary_by_office`; the second prints `ok: no loan capability in catalog`. If a loan capability already exists, the loan rows must be re-verdicted — stop and re-audit.

- [ ] **Step 2: Append the savings gap rows**

Append to `docs/product/analyst-question-inventory.md`:
```markdown
## Known gaps (savings/client) — feed W-A3

These are real analyst phrasings the catalog does not yet fully answer. They are listed so W-A3 can close them; they are not part of the ≥25 covered inventory above.

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| G1 | Total keseluruhan charge yang pernah dikenakan pada charge ini berapa? | What is the total ever levied on this savings charge across all occurrences? | charge name, amount levied total (paid+waived+writtenoff+outstanding), amount outstanding | none | `savings_pending_charges_clients` | partial |
| G2 | Charge tabungan mana yang benar-benar sudah lewat jatuh tempo (overdue) saja? | Which savings charges are strictly overdue (past due date) only? | client identity, charge name, due date, days overdue (>0 only) | none | `savings_pending_charges_clients` | partial |
```

Rationale (do not add to the doc — this is plan context): G1 needs `amount_levied_total`, absent today (A.1.3 Finding 1, roadmap Bundle 4). G2 needs a strictly-overdue variant; the current capability deliberately does **not** filter on `charge_due_date` (A.1.3 Finding 2), so an overdue-only reading is a separate variant W-A3 may add.

- [ ] **Step 3: Append the loan section**

Append to `docs/product/analyst-question-inventory.md`:
```markdown
## Loan domain — owned by issue 008 (all `missing` in 007)

The catalog has zero loan capabilities today. These questions are enumerated so the gap stays visible from 007; they are implemented under `docs/issues/active/008-loan-domain-analyst-capabilities.md`. Field sets are indicative — 008 resolves the arrears-source, office-scope, and `loan_status_id` design questions before finalizing them.

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id (reserved in 008) | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | Nasabah mana yang pinjamannya menunggak? | Which clients have loans in arrears? | client identity, loan account, days in arrears, overdue amount, currency | none | `loans_in_arrears_clients` | missing |
| L2 | Angsuran mana yang sudah lewat jatuh tempo? | Which loan installments are overdue? | loan account, installment due date, days overdue, amount overdue | none | `loan_overdue_installments` | missing |
| L3 | Berapa sisa pokok pinjaman per nasabah? | What is the outstanding loan balance per client? | client identity, loan account, outstanding principal, outstanding total, currency | none | `loan_outstanding_balances_clients` | missing |
| L4 | Nasabah mana yang masih punya charge pinjaman belum dibayar? | Which clients have unpaid loan charges? | client identity, charge name, amount outstanding, currency | none | `loan_unpaid_charges_clients` | missing |
| L5 | Tampilkan ringkasan portofolio pinjaman per kantor | Show the loan portfolio summary per office | office, loan count, outstanding total, arrears total, currency | none | `loan_portfolio_summary_by_office` | missing |
```

- [ ] **Step 4: Validate the appended sections exist**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
grep -cE '^\| G[0-9]+ \|' docs/product/analyst-question-inventory.md
grep -cE '^\| L[0-9]+ \|' docs/product/analyst-question-inventory.md
grep -c 'issue 008\|008-loan-domain' docs/product/analyst-question-inventory.md
```
Expected: `2` gap rows, `5` loan rows, and at least `1` reference to issue 008.

---

## Task 4: Final validation pass — count, capability-id existence, legend integrity

Machine-check the finished doc against the constraints so no invented id or dropped row ships.

**Files:**
- Read only. Modify none (unless a check fails).

- [ ] **Step 1: Confirm ≥25 numbered inventory questions**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
n=$(grep -cE '^\| [0-9]+ \|' docs/product/analyst-question-inventory.md)
echo "numbered questions: $n"
test "$n" -ge 25 && echo "PASS: >=25" || echo "FAIL: fewer than 25"
```
Expected: `numbered questions: 29` and `PASS: >=25`.

- [ ] **Step 2: Confirm every non-loan capability id in the doc exists in the catalog**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
grep -oE '`[a-z_]+`' docs/product/analyst-question-inventory.md \
  | tr -d '`' | grep -E '^(savings_|client_|organization_|office_list_basic)' | sort -u \
  | while read -r id; do
      grep -rl "^id: $id$" knowledge/capabilities >/dev/null \
        && echo "ok  $id" || echo "MISSING $id"
    done
```
Expected: every line prints `ok  <id>`; zero `MISSING`. A `MISSING` means the doc cites a capability that does not exist — fix the row.

- [ ] **Step 3: Confirm loan ids match issue 008's reserved set and are marked `missing`**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
for id in loans_in_arrears_clients loan_overdue_installments \
          loan_outstanding_balances_clients loan_unpaid_charges_clients \
          loan_portfolio_summary_by_office; do
  grep -q "\`$id\`" docs/product/analyst-question-inventory.md \
    && grep -q "$id" docs/issues/active/008-loan-domain-analyst-capabilities.md \
    && echo "ok  $id" || echo "PROBLEM $id"
done
grep -E '^\| L[0-9]+ \|' docs/product/analyst-question-inventory.md | grep -vc 'missing' \
  | xargs -I{} sh -c 'test {} -eq 0 && echo "ok: all loan rows missing" || echo "FAIL: a loan row is not missing"'
```
Expected: `ok  <id>` for all five, then `ok: all loan rows missing`.

- [ ] **Step 4: Confirm the legend, verdict values, and business-date framing are present**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
grep -q '## Legend' docs/product/analyst-question-inventory.md && echo "ok: legend"
grep -qE 'covered' docs/product/analyst-question-inventory.md \
  && grep -qE 'partial' docs/product/analyst-question-inventory.md \
  && grep -qE 'missing' docs/product/analyst-question-inventory.md && echo "ok: all three verdicts used"
grep -q 'business_today\|business date\|authorized_scope' docs/product/analyst-question-inventory.md \
  && echo "ok: default/no-clarification framing present"
```
Expected: `ok: legend`, `ok: all three verdicts used`, `ok: default/no-clarification framing present`.

- [ ] **Step 5: Confirm no source file outside the doc changed**

Run:
```bash
cd /Users/tabrezakhlaque/project/personal/rust/projects/ai_report
git status --porcelain -- 'crates' 'knowledge' 'queries' 'migrations'
echo "--- doc status ---"
git status --porcelain -- docs/product/analyst-question-inventory.md
```
Expected: the first prints **nothing** (no code/YAML/SQL/migration touched by this bundle); the second shows the new doc as untracked (`??`) or added. If the first prints anything, this bundle overstepped its documentation-only scope — revert those changes.

---

## Completion gate

B3 is complete only when: `docs/product/analyst-question-inventory.md` exists with ≥25 numbered questions across savings/client/organization, each carrying an Indonesian phrasing, an English phrasing, a required field set, a user-required-param note, a real capability id, and a `covered`/`partial`/`missing` verdict; the two savings gap rows and five loan rows (all `missing`, cross-referencing issue 008) are present; every non-loan capability id resolves to a file under `knowledge/capabilities/**`; all five loan ids match issue 008's reserved set; and `git status` shows no change under `crates/`, `knowledge/`, `queries/`, or `migrations/`. No commit step is performed.

## Out of scope (do not do here)

- **W-A4 per-capability default review table.** The issue mentions appending a date/limit default-decision table to this same doc; the program roadmap assigns that to Bundle 4 (Savings catalog). Do not author it here — it depends on the SQL rewrite that Bundle 4 performs.
- **W-A3 gap closure.** Closing G1/G2 (or any loan capability) is code work owned by Bundle 4 / issue 008. This bundle only records the gaps.
- Any `.rs`, `.yaml`, `.sql`, or migration edit.
