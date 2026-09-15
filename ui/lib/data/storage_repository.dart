import '../models/event.dart';
import '../models/analysis.dart';

/// 存储仓库抽象接口
/// 定义所有存储操作，方便未来切换实现
abstract class StorageRepository {
  /// 初始化存储
  Future<void> initialize();

  /// 记录新事件
  Future<Event> recordEvent(String rawText);

  /// 列出所有事件
  Future<List<Event>> listEvents();

  /// 列出分析结果
  Future<List<Analysis>> listAnalyses();

  /// 获取 AI 提供商信息
  Future<String?> getAiProvider();

  /// 触发 AI 分析
  /// 返回 "success" 或错误消息
  Future<String> triggerAnalysis();
}
