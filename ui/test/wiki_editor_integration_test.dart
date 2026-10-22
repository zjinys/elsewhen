// M3 集成测试（§6 / §7）：
//  1. 详情页正文从 MarkdownView 换为 AppFlowy 编辑器（浏览态只读渲染）；
//  2. 「编辑正文」→ 编辑态 →「完成」保存：documentToMarkdown → saveWikiPageContent；
//  3. Ctrl+S 保存（HardwareKeyboard 全局监听）；
//  4. 「取消」丢弃未保存修改；
//  5. 保存失败：留在编辑态并展示错误条；
//  6. 素材页（kind=source/note）不显示编辑入口；
//  7. 聊天移入正文尾部对话块（页尾仅一个聊天面板），footer 不再有独立聊天面板；
//  8. wikilink 点击跳转目标页 tab；
//  9. 关闭有未保存修改的 tab 前弹确认。
import 'dart:async';

import 'package:appflowy_editor/appflowy_editor.dart';
import 'package:collection/collection.dart';
import 'package:flutter/gestures.dart';
import 'package:flutter/material.dart';
import 'package:flutter/rendering.dart';
import 'package:flutter/services.dart';
import 'package:flutter_riverpod/flutter_riverpod.dart';
import 'package:flutter_test/flutter_test.dart';

import 'package:elsewhen_ui/bridge/api.dart' show EntityFactDto;
import 'package:elsewhen_ui/bridge/rust_bridge_repository.dart';
import 'package:elsewhen_ui/models/conversation.dart';
import 'package:elsewhen_ui/models/relation.dart';
import 'package:elsewhen_ui/models/wiki_page.dart';
import 'package:elsewhen_ui/providers/wiki_provider.dart';
import 'package:elsewhen_ui/widgets/wiki_ai_chat_panel.dart';
import 'package:elsewhen_ui/widgets/markdown_view.dart';
import 'package:elsewhen_ui/widgets/wiki_derivatives.dart';
import 'package:elsewhen_ui/widgets/wiki_page_detail_view.dart';
import 'package:elsewhen_ui/wiki/wiki_content_editor.dart';

part 'wiki_editor_integration_parts/wiki_editor_fake.dart';
part 'wiki_editor_integration_parts/wiki_editor_core_steps.dart';
part 'wiki_editor_integration_parts/wiki_editor_nav_steps.dart';
part 'wiki_editor_integration_parts/wiki_editor_dirty_tab_steps.dart';
part 'wiki_editor_integration_parts/wiki_editor_fake_class.dart';

void main() {
  group('wiki 详情页 · M3 编辑器集成', () {
    _m3CoreTests();
    _m3NavTests();
    _m3DirtyTabTests();
  });
}
