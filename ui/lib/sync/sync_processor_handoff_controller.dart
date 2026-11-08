import '../l10n/notice_messages.dart';

import 'package:flutter/foundation.dart';

import '../bridge/api.dart' as api;

/// 加工移交（P4d）的 FRB 入口（可注入测试替身）。
///
/// 对应 `src/api/sync.rs::get_processor_handoff_state` /
/// `yield_sync_processor` / `approve_processor_handoff` /
/// `confirm_processor_takeover`。所有写操作在 Rust 侧失败关闭：静默未达成、
/// frontier 未追平、非 owner 审批都会抛出同步错误码。
abstract interface class ProcessorHandoffApi {
  Future<api.ProcessorHandoffStateDto> getState();

  Future<api.ProcessingYieldProofDto> yieldProcessor({
    required String toDeviceId,
  });

  Future<void> approve({required api.ProcessingYieldProofDto proof});

  Future<void> confirmTakeover();
}

class FrbProcessorHandoffApi implements ProcessorHandoffApi {
  const FrbProcessorHandoffApi();

  @override
  Future<api.ProcessorHandoffStateDto> getState() =>
      api.getProcessorHandoffState();

  @override
  Future<api.ProcessingYieldProofDto> yieldProcessor({
    required String toDeviceId,
  }) => api.yieldSyncProcessor(toDeviceId: toDeviceId);

  @override
  Future<void> approve({required api.ProcessingYieldProofDto proof}) =>
      api.approveProcessorHandoff(proof: proof);

  @override
  Future<void> confirmTakeover() => api.confirmProcessorTakeover();
}

/// 设置页的加工移交状态与显式动作。
///
/// 只展示状态与两个显式按钮（让出 / 接管），绝不自动推进任何状态机步骤；
/// 让出后会尝试 owner 审批（仅当本机是 owner），其余等待目标设备自行确认。
/// 失败保留旧视图并显示错误，绝不静默清空。
class SyncProcessorHandoffController extends ChangeNotifier {
  SyncProcessorHandoffController({ProcessorHandoffApi? bridge})
    : _bridge = bridge ?? const FrbProcessorHandoffApi();

  final ProcessorHandoffApi _bridge;

  api.ProcessorHandoffStateDto? state;
  String? error;
  bool busy = false;
  bool _disposed = false;

  void _changed() {
    if (!_disposed) notifyListeners();
  }

  Future<void> refresh() async {
    try {
      state = await _bridge.getState();
      error = null;
    } catch (e) {
      error = handoffErrorText(e.toString());
    }
    _changed();
  }

  /// 旧加工端让出加工权。[isOwnerDevice] 为真时（owner 与旧加工端同机），
  /// 生成证明后立即由本机 owner 审批签发后继名册。
  Future<void> yieldTo({
    required String toDeviceId,
    required bool isOwnerDevice,
  }) async {
    if (toDeviceId.trim().isEmpty) {
      error = appNotice('l6EnterTheTargetDeviceIdb0da4');
      _changed();
      return;
    }
    busy = true;
    error = null;
    _changed();
    try {
      final proof = await _bridge.yieldProcessor(toDeviceId: toDeviceId.trim());
      if (isOwnerDevice) {
        await _bridge.approve(proof: proof);
      }
      await refresh();
    } catch (e) {
      error = handoffErrorText(e.toString());
    } finally {
      busy = false;
      _changed();
    }
  }

  /// 接管目标确认：仅在 Rust 侧 frontier 已追平时成功。
  Future<void> confirm() async {
    busy = true;
    error = null;
    _changed();
    try {
      await _bridge.confirmTakeover();
      await refresh();
    } catch (e) {
      error = handoffErrorText(e.toString());
    } finally {
      busy = false;
      _changed();
    }
  }

  @override
  void dispose() {
    _disposed = true;
    super.dispose();
  }
}

/// 同步错误码 → 设置页可读文案；未识别的原样透传（绝不吃掉细节）。
String handoffErrorText(String raw) {
  // FRB 异常可能带 anyhow 前缀/堆栈，仅做包含匹配。
  String? matched;
  for (final entry in _handoffErrors.entries) {
    if (raw.contains(entry.key)) {
      matched = entry.value;
      break;
    }
  }
  return matched ?? raw;
}

final Map<String, String> _handoffErrors = {
  'sync_membership_not_processor': appNotice(
    'l6ThisDeviceIsNotTheProcessor82939',
  ),
  'sync_membership_owner_required': appNotice(
    'l6OnlyTheDatasetOwnerCanApprovec5d36',
  ),
  'sync_background_processing_must_finish_before_enable': appNotice(
    'l6WaitForThisDeviceSActive1e382',
  ),
  'sync_frontier_lagging': appNotice('l6WaitForSyncToReachThe58a6b'),
  'sync_processor_not_in_pending_takeover': appNotice(
    'l6NoProcessorHandoffAwaitsThisDevice12c2e',
  ),
  'sync_handoff_signature_invalid': appNotice(
    'l6TheHandoffProofSignatureIsInvalid82d09',
  ),
  'sync_handoff_revision_invalid': appNotice(
    'l6HandoffProofInformationIsOutsideIts3c0a0',
  ),
  'sync_handoff_revision_mismatch': appNotice('l6TheProofDoesNotMatchThe90664'),
  'sync_handoff_source_mismatch': appNotice('l6TheProofDoesNotMatchThe7ec43'),
  'sync_handoff_target_missing': appNotice('l6TheTargetIsNotAnActivea00bf'),
  'sync_handoff_old_processor_inactive': appNotice(
    'l6TheOriginalProcessorIsNoLongerd44b7',
  ),
  'sync_handoff_dataset_mismatch': appNotice(
    'l6TheHandoffProofBelongsToA7b34b',
  ),
  'sync_handoff_self_transfer_invalid': appNotice(
    'l6AiAnalysisCannotBeTransferredTod5ffa',
  ),
  'sync_handoff_proof_target_mismatch': appNotice(
    'l6TheHandoffProofDoesNotMatch31e8f',
  ),
  'sync_handoff_proof_required': appNotice(
    'l6HandoffProofIsMissingTheDevice668d5',
  ),
  'sync_handoff_proof_source_mismatch': appNotice(
    'l6TheProofSSourceDoesNot02b44',
  ),
  'sync_handoff_source_missing': appNotice('l6TheHandoffProofHasNoSource9502a'),
  'sync_handoff_target_mismatch': appNotice(
    'l6TheHandoffProofTargetsADifferent2a815',
  ),
  'sync_processing_handoff_required': appNotice(
    'l6ProcessorHandoffProofIsMissingThis0e9df',
  ),
  'sync_handoff_unexpected_without_processor_change': appNotice(
    'l6TheRosterIncludesHandoffProofWithout363f4',
  ),
  'sync_runtime_locked': appNotice('l6UnlockThisDeviceSSyncKeyse19a6'),
  'sync_not_initialized': appNotice('l6ThisDeviceSSyncIdentityIs5c640'),
};
