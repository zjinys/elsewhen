# Agent Note: 决策辅助上下文注入

Status: implemented

## Problem

事件分析只看当前原文，无法调用用户已经确认的规则和知识页，因此 follow-up 建议缺乏个人上下文。

## Decision

在异步事件分析 prompt 中注入有界的已确认规则和最近知识页摘要。上下文只读、数量有上限，并明确提示模型不得据此臆测；任何待办、知识页或关系写入仍走既有工具确认门。

## Alternatives considered

- 让模型自由读取全库：上下文不可控，容易泄露无关内容并增加 token 成本。
- 只注入当前事件：无法形成可复用经验和历史回溯。
- 直接根据模型建议写入：违反高影响写入确认边界。

## Verification

- Rust 测试确认上下文只包含 active 规则和 active 知识页摘要。
- `cargo test --lib` 全部通过。

## Consequences

分析结果开始具备个人上下文，但当前仍是建议输出；接受、忽略和改写反馈闭环留给 Phase 4C 后续切片。
