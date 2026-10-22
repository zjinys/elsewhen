// Copyright 2019 The Fuchsia Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE file.

// coverage:ignore-file

import 'package:flutter/foundation.dart';

import 'item_positions_listener.dart';

/// Internal implementation of [ItemPositionsListener].
class ItemPositionsNotifier implements ItemPositionsListener {
  @override
  final ValueNotifier<Iterable<ItemPosition>> itemPositions = ValueNotifier([]);

  bool _disposed = false;

  /// elsewhen 补丁：`PositionedList` 的 post-frame 回调可能在列表销毁后
  /// 才执行，原实现会向已 dispose 的 [itemPositions] 写值并抛
  /// "A ValueNotifier was used after being disposed"。
  bool get isDisposed => _disposed;

  void dispose() {
    if (_disposed) return;
    _disposed = true;
    itemPositions.dispose();
  }
}
