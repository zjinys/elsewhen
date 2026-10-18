#!/usr/bin/env python3
"""缓存命中率全量报表（只读）。

数据源：chat_run_requests（迁移 40/43 的观测列：prompt_tokens /
prompt_cached_tokens / pruned_tool_results）+ conversation_page_marks
（迁移 46 翻篇）。

用法：scripts/cache_report.py [db路径]
默认库：~/.local/share/elsewhen/elsewhen.db

只读打开（mode=ro），不会写库。cached_tokens 依赖 provider 返回
usage.prompt_tokens_details.cached_tokens——usage_source != 'provider'
的行不算命中率（unknown 多为流式中断/旧版本，见 usage_source 列）。
"""

import sqlite3
import sys
from pathlib import Path

DEFAULT_DB = Path.home() / ".local/share/elsewhen/elsewhen.db"


def rows(db, sql, args=()):
    return db.execute(sql, args).fetchall()


def pct(n, d):
    return f"{100.0 * n / d:.1f}%" if d else "—"


def main():
    path = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_DB
    db = sqlite3.connect(f"file:{path}?mode=ro", uri=True)
    db.row_factory = sqlite3.Row

    print(f"== 缓存命中率报表 ==  {path}")
    print()

    # ---- 总览 ----
    overall = db.execute(
        """SELECT count(*) n,
                  sum(prompt_tokens) pt,
                  sum(prompt_cached_tokens) ct,
                  sum(completion_tokens) out_t,
                  sum(pruned_tool_results) pruned
           FROM chat_run_requests
           WHERE status='completed' AND prompt_tokens>0"""
    ).fetchone()
    prov = db.execute(
        """SELECT count(*) n, sum(prompt_tokens) pt, sum(prompt_cached_tokens) ct
           FROM chat_run_requests
           WHERE status='completed' AND usage_source='provider' AND prompt_tokens>0"""
    ).fetchone()
    print("总览（completed 且 prompt>0）")
    print(f"  请求 {overall['n']} 条 | prompt {overall['pt'] or 0:,} tok | "
          f"completion {overall['out_t'] or 0:,} tok | 清剪工具结果 {overall['pruned'] or 0} 次")
    print(f"  可算命中率（usage=provider）：{prov['n']} 条 | "
          f"prompt {prov['pt'] or 0:,} | cached {prov['ct'] or 0:,} | "
          f"命中率 {pct(prov['ct'] or 0, prov['pt'] or 0)}")
    print(f"  其余 {overall['n'] - prov['n']} 条 usage 缺失（流式中断/旧版本），不计入")
    print()

    # ---- 按模型 ----
    print("按模型（usage=provider）")
    print(f"  {'模型':<34}{'请求':>5}{'命中率':>9}{'均prompt':>11}")
    for r in rows(db, """SELECT coalesce(model,'(null)') m, count(*) n,
                                sum(prompt_tokens) pt, sum(prompt_cached_tokens) ct
                         FROM chat_run_requests
                         WHERE status='completed' AND usage_source='provider' AND prompt_tokens>0
                         GROUP BY 1 ORDER BY n DESC"""):
        print(f"  {r['m']:<34}{r['n']:>5}{pct(r['ct'] or 0, r['pt']):>9}{(r['pt'] or 0)//r['n']:>11,}")
    print()

    # ---- 按日 ----
    print("按日（usage=provider；cached_tokens 自 2026-10-18 起采集）")
    for r in rows(db, """SELECT substr(started_at,1,10) d, count(*) n,
                                sum(prompt_tokens) pt, sum(prompt_cached_tokens) ct
                         FROM chat_run_requests
                         WHERE status='completed' AND usage_source='provider' AND prompt_tokens>0
                         GROUP BY 1 ORDER BY 1 DESC LIMIT 14"""):
        print(f"  {r['d']}  {r['n']:>4} 请求  prompt {r['pt'] or 0:>9,}  命中 {pct(r['ct'] or 0, r['pt'] or 0)}")
    print()

    # ---- 翻篇前后 ----
    marks = rows(db, """SELECT m.conversation_id, m.kind, m.created_at, m.boundary_message_id
                        FROM conversation_page_marks m ORDER BY m.created_at""")
    print(f"翻篇前后对比（迁移 46）：共 {len(marks)} 条翻篇记录")
    if not marks:
        print("  （尚无翻篇记录——出第一条数据后这里自动给出前后窗口对比）")
        print("  对比口径：翻篇时刻前后各 24h、同会话 completed 请求的")
        print("  prompt 均值与命中率；预期翻篇后 prompt 显著变小。")
    for m in marks:
        conv = m["conversation_id"]
        t = m["created_at"]
        for label, clause in (("翻篇前", "started_at < ? AND started_at >= datetime(?, '-24 hours')"),
                              ("翻篇后", "started_at >= ? AND started_at <= datetime(?, '+24 hours')")):
            r = db.execute(
                f"""SELECT count(*) n, sum(prompt_tokens) pt, sum(prompt_cached_tokens) ct
                    FROM chat_run_requests
                    WHERE conversation_id=? AND status='completed'
                      AND prompt_tokens>0 AND {clause}""",
                (conv, t, t),
            ).fetchone()
            print(f"  [{m['kind']}] {t} {label}24h：{r['n']} 请求，prompt 均 "
                  f"{(r['pt'] or 0)//r['n'] if r['n'] else 0:,}，命中 {pct(r['ct'] or 0, r['pt'] or 0)}")
    print()

    # ---- 清剪（pruned tool results）----
    pr = db.execute(
        """SELECT count(*) n, sum(pruned_tool_results) p FROM chat_run_requests
           WHERE pruned_tool_results>0"""
    ).fetchone()
    print(f"观测列（迁移 43）：清剪过工具结果的请求 {pr['n']} 条，累计清剪 {pr['p'] or 0} 条")


if __name__ == "__main__":
    main()
