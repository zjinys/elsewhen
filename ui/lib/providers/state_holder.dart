import 'package:flutter_riverpod/flutter_riverpod.dart';

/// StateProvider 的 Notifier 化替身：带显式写入方法的值容器。
///
/// Riverpod 3 把 StateProvider 移入 legacy 入口；简单值状态统一收敛到本类：
/// - `ref.watch(p)` 读值（与 StateProvider 一致）；
/// - `ref.read(p.notifier).set(v)` 写入（替代 `..state = v`，
///   Notifier 的 state setter 是 protected，外部只能走方法）；
/// - `ref.read(p.notifier).update(f)` 读旧写新。
class StateHolder<T> extends Notifier<T> {
  StateHolder(this._initial);

  final T _initial;

  @override
  T build() => _initial;

  void set(T value) => state = value;

  void update(T Function(T current) fn) => state = fn(state);

  /// 容器销毁后为 false。Riverpod 3 下销毁后写 state 抛 UnmountedRefException
  /// （内部类型，不可捕获），异步延迟写入前先查此标记。
  bool get isMounted => ref.mounted;
}
