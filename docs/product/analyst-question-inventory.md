# Analyst Question Inventory

**Purpose:** Map real bilingual analyst questions to the current knowledge catalog. Each row records the required result fields, whether the analyst must supply a parameter, and the coverage verdict.

**Scope:** Savings, client, and organization are owned by issue 007. Loan questions remain visible here but are owned by [issue 008](../issues/active/008-loan-domain-analyst-capabilities.md).

**Verified:** 2026-07-27 against `knowledge/capabilities/**`: 30 approved capabilities (11 savings, 10 client, 9 organization), and no loan capability.

## Legend

| Verdict | Meaning |
| --- | --- |
| `covered` | One capability returns the required field set with sane defaults. |
| `partial` | A capability answers the intent but lacks a field or variant; feeds W-A3. |
| `missing` | No approved capability answers it. |

**User-required param:** `none` means policies supply `authorized_scope`, `business_today`, and any limit default. Only `client_name_lookup` requires the user to supply a name.

## Savings domain

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| 1 | Berapa saldo total tabungan aktif saat ini? | Total active savings balance now? | account count, balance, currency | none | `savings_balance_summary` | covered |
| 2 | Nasabah mana masih punya charge belum dibayar, beserta due date, hari terlambat, dibayar, dan sisa? | Which clients have unpaid savings charges, due date, overdue days, paid amount, and balance? | client, charge, due date, days overdue, paid, waived, outstanding, currency | none | `savings_pending_charges_clients` | covered |
| 3 | Berapa total penarikan bulan ini? | Total withdrawals this month? | period, amount, currency | none | `savings_withdrawal_total` | covered |
| 4 | Berapa total setoran bulan ini? | Total deposits this month? | period, amount, currency | none | `savings_deposit_total` | covered |
| 5 | Siapa penarik terbesar bulan ini? | Who made the largest withdrawals this month? | client, amount, date, rank | none | `savings_withdrawal_top_n` | covered |
| 6 | Siapa penyetor terbesar hari ini? | Who made the largest deposits today? | client, amount, date, rank | none | `savings_deposit_top_n` | covered |
| 7 | Tunjukkan aktivitas tabungan minggu ini | Show savings activity this week | account, transaction, amount, date | none | `savings_activity_list` | covered |
| 8 | Setoran per bulan tahun ini | Monthly deposits this year | month, amount, currency | none | `savings_deposit_monthly_breakdown` | covered |
| 9 | Penarikan per bulan tahun ini | Monthly withdrawals this year | month, amount, currency | none | `savings_withdrawal_monthly_breakdown` | covered |
| 10 | Setoran terbesar setiap bulan | Largest deposit each month | month, client, amount | none | `savings_deposit_monthly_top_n` | covered |

## Client domain

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| 11 | Tunjukkan nasabah baru diaktivasi | Show recently activated clients | client, activation date, office | none | `client_list_recent` | covered |
| 12 | Ada nama Tony di client? | Is there a client named Tony? | client, office, status | name | `client_name_lookup` | covered |
| 13 | Aktivasi nasabah tiap bulan tahun lalu | Client activations each month last year | month, count | none | `client_activation_monthly_breakdown` | covered |
| 14 | Kantor teratas aktivasi nasabah bulan ini | Top offices by new client activations | office, count, rank | none | `client_activation_top_n_offices` | covered |
| 15 | Berikan 5 client sembarang | Give a random sample of 5 clients | client, office | none | `client_random_sample` | covered |
| 16 | Ringkasan lifecycle nasabah | Client lifecycle summary | status, count | none | `client_lifecycle_summary` | covered |
| 17 | Jumlah nasabah aktif per kantor | Active clients per office | office, lifecycle counts | none | `client_summary_by_office` | covered |
| 18 | Nasabah dengan setoran terbesar | Top clients by deposit volume | client, volume, rank, currency | none | `client_top_n_by_deposit_volume` | covered |
| 19 | Nasabah dengan rekening tabungan terbanyak | Clients with most savings accounts | client, account count, rank | none | `client_top_n_by_savings_account_count` | covered |
| 20 | Nasabah dengan saldo tertinggi | Clients with highest savings balance | client, balance, rank, currency | none | `client_top_n_by_savings_balance` | covered |

## Organization domain

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| 21 | Ringkasan hierarki kantor | Office hierarchy summary | roots, leaves, depth | none | `organization_hierarchy_summary` | covered |
| 22 | Kantor paling aktif bulan ini | Offices with most transactions | office, count, rank | none | `organization_office_activity_ranking` | covered |
| 23 | Jumlah nasabah per kantor | Client counts per office | office, client count | none | `organization_office_client_summary` | covered |
| 24 | Kantor tanpa aktivitas kuartal ini | Offices with no activity this quarter | office, last activity | none | `organization_office_dormant` | covered |
| 25 | Pohon hierarki kantor | Office hierarchy tree | office, parent, depth | none | `organization_office_hierarchy_tree` | covered |
| 26 | Daftar kantor pada scope saya | Offices in my authorized scope | office id, name | none | `office_list_basic` | covered |
| 27 | Kantor dibuka tiap bulan | Offices opened each month | month, count | none | `organization_office_opening_monthly_breakdown` | covered |
| 28 | Kantor dengan saldo terbesar | Offices with greatest savings balance | office, balance, rank, currency | none | `organization_office_savings_summary` | covered |
| 29 | Ringkasan kantor dan staf aktif | Office summary with active staff | office, staff, client count | none | `organization_office_summary` | covered |

## Known gaps (savings/client) — feed W-A3

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| G1 | Total charge yang pernah dikenakan berapa? | What is the total ever levied on this savings charge? | charge, levied total, outstanding | none | `savings_pending_charges_clients` | partial |
| G2 | Charge mana yang benar-benar overdue saja? | Which savings charges are strictly overdue only? | client, charge, due date, days overdue | none | `savings_pending_charges_clients` | partial |

## Loan domain — owned by issue 008 (all `missing` in 007)

| # | Indonesian phrasing | English phrasing | Required field set | User-required param? | Capability id (reserved in 008) | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| L1 | Nasabah mana pinjamannya menunggak? | Which clients have loans in arrears? | client, loan, arrears, currency | none | `loans_in_arrears_clients` | missing |
| L2 | Angsuran mana lewat jatuh tempo? | Which installments are overdue? | loan, due date, overdue days, amount | none | `loan_overdue_installments` | missing |
| L3 | Sisa pokok pinjaman per nasabah | Outstanding loan balance per client | client, loan, principal, total, currency | none | `loan_outstanding_balances_clients` | missing |
| L4 | Charge pinjaman belum dibayar | Clients with unpaid loan charges | client, charge, outstanding, currency | none | `loan_unpaid_charges_clients` | missing |
| L5 | Ringkasan portofolio pinjaman per kantor | Loan portfolio summary per office | office, loan count, outstanding, arrears, currency | none | `loan_portfolio_summary_by_office` | missing |

## W-A4 temporal & limit default decisions (E4)

Point-in-time reports retain `business_today`. Month-grouped reports use the trailing 12 months; single-period reports use month-to-date. Genuine rankings default to 10 rows, while analyst detail lists remain `unbounded` under a `hard_cap`. Derived `amount_levied_total`, `days_overdue`, and `charge_timing_enum` are `public_business`: none identifies a person.

| Capability | Date class / action | Limit action |
| --- | --- | --- |
| `savings_balance_summary` | point-in-time / business_today | none |
| `savings_pending_charges_clients` | point-in-time / business_today | unbounded, hard_cap 10000 |
| `savings_deposit_total` | rolling single / start_of_month(business_today) | none |
| `savings_withdrawal_total` | rolling single / start_of_month(business_today) | none |
| `savings_deposit_top_n` | rolling single / start_of_month(business_today) | default 10, hard_cap retained |
| `savings_withdrawal_top_n` | rolling single / start_of_month(business_today) | default 10, hard_cap retained |
| `savings_activity_list` | rolling single / start_of_month(business_today) | unbounded, hard_cap retained |
| `savings_deposit_monthly_breakdown` | rolling monthly / business_today - 12m | none |
| `savings_withdrawal_monthly_breakdown` | rolling monthly / business_today - 12m | none |
| `savings_deposit_monthly_top_n` | rolling monthly / business_today - 12m | default 10, hard_cap retained |
| `savings_withdrawal_monthly_top_n` | rolling monthly / business_today - 12m | default 10, hard_cap retained |
| `client_list_recent` | point-in-time / business_today | unbounded, hard_cap retained |
| `client_name_lookup` | no date | user-required name |
| `client_lifecycle_summary` | point-in-time / business_today | none |
| `client_activation_monthly_breakdown` | rolling monthly / business_today - 12m | none |
| `client_activation_top_n_offices` | rolling single / start_of_month(business_today) | default 10, hard_cap retained |
| `client_random_sample` | point-in-time / business_today | default/hard_cap retained |
| `client_summary_by_office` | point-in-time / business_today | none |
| `client_top_n_by_deposit_volume` | rolling single / start_of_month(business_today) | default 10, hard_cap retained |
| `client_top_n_by_savings_account_count` | point-in-time / business_today | default 10, hard_cap retained |
| `client_top_n_by_savings_balance` | point-in-time / business_today | default 10, hard_cap retained |
| `organization_hierarchy_summary` | no date | none |
| `organization_office_activity_ranking` | rolling single / start_of_month(business_today) | default 10, hard_cap retained |
| `organization_office_client_summary` | point-in-time / business_today | none |
| `organization_office_dormant` | rolling single / start_of_month(business_today) | unbounded, hard_cap retained |
| `organization_office_hierarchy_tree` | no date | none |
| `office_list_basic` | no date | default/hard_cap retained |
| `organization_office_opening_monthly_breakdown` | rolling monthly / business_today - 12m | none |
| `organization_office_savings_summary` | point-in-time / business_today | default/hard_cap retained |
| `organization_office_summary` | point-in-time / business_today | none |
