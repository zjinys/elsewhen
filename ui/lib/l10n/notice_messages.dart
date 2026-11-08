import 'dart:convert';

import 'app_localizations.dart';
import '../utils/app_error.dart';

// Ephemeral controller messages carry a resource identity, never translated
// prose. Resolution happens at render time so pending states follow locale.
const _prefix = 'elsewhen-notice-v1:';
String appNotice(String key, [List<String> arguments = const []]) =>
    _prefix + jsonEncode({'key': key, 'arguments': arguments});
String appFailure(Object error, {String? key}) =>
    _prefix + jsonEncode({'diagnostic': error.toString(), 'key': ?key});
Map<String, dynamic>? _decode(Object? notice) {
  final text = notice?.toString() ?? '';
  if (!text.startsWith(_prefix) || text.length > 65536) return null;
  try {
    final value = jsonDecode(text.substring(_prefix.length));
    return value is Map<String, dynamic> ? value : null;
  } on FormatException {
    return null;
  }
}

Object noticeDiagnostic(Object notice) =>
    _decode(notice)?['diagnostic'] ?? notice;
String? resolveNotice(
  Object? notice,
  AppLocalizations strings, [
  int depth = 0,
]) {
  final value = _decode(notice);
  if (value == null || depth > 6) return null;
  if (value['diagnostic'] != null) {
    return AppError.decode(value['diagnostic'])?.message(strings) ??
        (value['key'] is String
            ? resolveNotice(
                appNotice(value['key'] as String),
                strings,
                depth + 1,
              )
            : null) ??
        strings.operationFailed(strings.l6Actionf3ea6);
  }
  final raw = value['arguments'];
  if (raw is! List || raw.any((v) => v is! String)) return null;
  final args = raw
      .cast<String>()
      .map((v) => resolveNotice(v, strings, depth + 1) ?? v)
      .toList();
  return switch (value['key']) {
    'nameRequiredError' when args.isEmpty => strings.nameRequiredError,
    'secretFieldsRequired' when args.isEmpty => strings.secretFieldsRequired,
    'aiProviderMissing' when args.isEmpty => strings.aiProviderMissing,
    'aiReplyFailedGeneric' when args.isEmpty => strings.aiReplyFailedGeneric,
    'taskMaterialChanged' when args.isEmpty => strings.taskMaterialChanged,
    'noSupplementTask' when args.isEmpty => strings.noSupplementTask,
    'savedButNotEligible' when args.isEmpty => strings.savedButNotEligible,
    'pageMissing' when args.isEmpty => strings.pageMissing,
    'connectionSucceeded' when args.isEmpty => strings.connectionSucceeded,
    'l6EnterTheTargetDeviceIdb0da4' when args.isEmpty =>
      strings.l6EnterTheTargetDeviceIdb0da4,
    'l6ThisDeviceIsNotTheProcessor82939' when args.isEmpty =>
      strings.l6ThisDeviceIsNotTheProcessor82939,
    'l6OnlyTheDatasetOwnerCanApprovec5d36' when args.isEmpty =>
      strings.l6OnlyTheDatasetOwnerCanApprovec5d36,
    'l6WaitForThisDeviceSActive1e382' when args.isEmpty =>
      strings.l6WaitForThisDeviceSActive1e382,
    'l6WaitForSyncToReachThe58a6b' when args.isEmpty =>
      strings.l6WaitForSyncToReachThe58a6b,
    'l6NoProcessorHandoffAwaitsThisDevice12c2e' when args.isEmpty =>
      strings.l6NoProcessorHandoffAwaitsThisDevice12c2e,
    'l6TheHandoffProofSignatureIsInvalid82d09' when args.isEmpty =>
      strings.l6TheHandoffProofSignatureIsInvalid82d09,
    'l6HandoffProofInformationIsOutsideIts3c0a0' when args.isEmpty =>
      strings.l6HandoffProofInformationIsOutsideIts3c0a0,
    'l6TheProofDoesNotMatchThe90664' when args.isEmpty =>
      strings.l6TheProofDoesNotMatchThe90664,
    'l6TheProofDoesNotMatchThe7ec43' when args.isEmpty =>
      strings.l6TheProofDoesNotMatchThe7ec43,
    'l6TheTargetIsNotAnActivea00bf' when args.isEmpty =>
      strings.l6TheTargetIsNotAnActivea00bf,
    'l6TheOriginalProcessorIsNoLongerd44b7' when args.isEmpty =>
      strings.l6TheOriginalProcessorIsNoLongerd44b7,
    'l6TheHandoffProofBelongsToA7b34b' when args.isEmpty =>
      strings.l6TheHandoffProofBelongsToA7b34b,
    'l6AiAnalysisCannotBeTransferredTod5ffa' when args.isEmpty =>
      strings.l6AiAnalysisCannotBeTransferredTod5ffa,
    'l6TheHandoffProofDoesNotMatch31e8f' when args.isEmpty =>
      strings.l6TheHandoffProofDoesNotMatch31e8f,
    'l6HandoffProofIsMissingTheDevice668d5' when args.isEmpty =>
      strings.l6HandoffProofIsMissingTheDevice668d5,
    'l6TheProofSSourceDoesNot02b44' when args.isEmpty =>
      strings.l6TheProofSSourceDoesNot02b44,
    'l6TheHandoffProofHasNoSource9502a' when args.isEmpty =>
      strings.l6TheHandoffProofHasNoSource9502a,
    'l6TheHandoffProofTargetsADifferent2a815' when args.isEmpty =>
      strings.l6TheHandoffProofTargetsADifferent2a815,
    'l6ProcessorHandoffProofIsMissingThis0e9df' when args.isEmpty =>
      strings.l6ProcessorHandoffProofIsMissingThis0e9df,
    'l6TheRosterIncludesHandoffProofWithout363f4' when args.isEmpty =>
      strings.l6TheRosterIncludesHandoffProofWithout363f4,
    'l6UnlockThisDeviceSSyncKeyse19a6' when args.isEmpty =>
      strings.l6UnlockThisDeviceSSyncKeyse19a6,
    'l6ThisDeviceSSyncIdentityIs5c640' when args.isEmpty =>
      strings.l6ThisDeviceSSyncIdentityIs5c640,
    'l6TransferThisDeviceSAiAnalysis9e01d' when args.isEmpty =>
      strings.l6TransferThisDeviceSAiAnalysis9e01d,
    'l6AnotherDeviceUpdatedTheRosterRefreshb7381' when args.isEmpty =>
      strings.l6AnotherDeviceUpdatedTheRosterRefreshb7381,
    'l6OnlyTheDatasetOwnerCanRevoke2ce42' when args.isEmpty =>
      strings.l6OnlyTheDatasetOwnerCanRevoke2ce42,
    'l6ThisDeviceSAccessHasAlreadyb76c9' when args.isEmpty =>
      strings.l6ThisDeviceSAccessHasAlreadyb76c9,
    'l6ThisDeviceIsNotInThe3f9c7' when args.isEmpty =>
      strings.l6ThisDeviceIsNotInThe3f9c7,
    'l6InvalidRosterRevisione21c2' when args.isEmpty =>
      strings.l6InvalidRosterRevisione21c2,
    'l6TheRosterHasReachedItsMemberee48e' when args.isEmpty =>
      strings.l6TheRosterHasReachedItsMemberee48e,
    'l6DeviceIdentitiesConflictInTheRosterea7bc' when args.isEmpty =>
      strings.l6DeviceIdentitiesConflictInTheRosterea7bc,
    'l6TheLocalPageChangedRefreshAnd4e980' when args.isEmpty =>
      strings.l6TheLocalPageChangedRefreshAnd4e980,
    'errorConflictResolved' when args.isEmpty => strings.errorConflictResolved,
    'l6ChannelAuthorizationOrRevisionChangedAutomatic779c0' when args.isEmpty =>
      strings.l6ChannelAuthorizationOrRevisionChangedAutomatic779c0,
    'l6TheHandoffRevisionOrIdentityCould284a1' when args.isEmpty =>
      strings.l6TheHandoffRevisionOrIdentityCould284a1,
    'l6TheRevisionAndReleaseOfThebd873' when args.isEmpty =>
      strings.l6TheRevisionAndReleaseOfThebd873,
    'l6TheHandoffStepIsIncompleteThe70ae7' when args.isEmpty =>
      strings.l6TheHandoffStepIsIncompleteThe70ae7,
    'l6Arg0Secondse612a' when args.length == 1 => strings.l6Arg0Secondse612a(
      args[0],
    ),
    'l6Arg0Milliseconds5f522' when args.length == 1 =>
      strings.l6Arg0Milliseconds5f522(args[0]),
    'l6TheSyncStepDidNotRespond28e88' when args.length == 1 =>
      strings.l6TheSyncStepDidNotRespond28e88(args[0]),
    'l6TheApprovedHandoffIsBeingCoordinated7e9e2' when args.isEmpty =>
      strings.l6TheApprovedHandoffIsBeingCoordinated7e9e2,
    'l6TheNativeHandoffRevisionCouldNot775d0' when args.isEmpty =>
      strings.l6TheNativeHandoffRevisionCouldNot775d0,
    'l6PreviewDidNotConnectToThed4c19' when args.isEmpty =>
      strings.l6PreviewDidNotConnectToThed4c19,
    'l6EnterADeviceNameAndExplicitly802c8' when args.isEmpty =>
      strings.l6EnterADeviceNameAndExplicitly802c8,
    'l6JoinRequestDeliveredVerifyThisDevice8e98f' when args.isEmpty =>
      strings.l6JoinRequestDeliveredVerifyThisDevice8e98f,
    'l6NoUnfinishedEnrollmentStagingEntriesaa853' when args.isEmpty =>
      strings.l6NoUnfinishedEnrollmentStagingEntriesaa853,
    'l6RecoveryReadsTheOriginalInstallationS770c9' when args.isEmpty =>
      strings.l6RecoveryReadsTheOriginalInstallationS770c9,
    'l6StagingEntryDeletedButKeyCleanup3ce20' when args.isEmpty =>
      strings.l6StagingEntryDeletedButKeyCleanup3ce20,
    'l6StagingEntryArg0Deleted0f067' when args.length == 1 =>
      strings.l6StagingEntryArg0Deleted0f067(args[0]),
    'l6OriginalInstallationProofResentWaitingForee8e7' when args.isEmpty =>
      strings.l6OriginalInstallationProofResentWaitingForee8e7,
    'l6OriginalRequestResentWaitingForExplicitb70c0' when args.isEmpty =>
      strings.l6OriginalRequestResentWaitingForExplicitb70c0,
    'l6InstallationProofDeliveredWaitingForAuthorization69cc4'
        when args.isEmpty =>
      strings.l6InstallationProofDeliveredWaitingForAuthorization69cc4,
    'l6NoUsableApprovalYetVerifyAnd0b6c4' when args.isEmpty =>
      strings.l6NoUsableApprovalYetVerifyAnd0b6c4,
    'l6TheSameInstallationProofWasResent1abc4' when args.isEmpty =>
      strings.l6TheSameInstallationProofWasResent1abc4,
    'l6EnrollmentStoppedStagedDataAndKeys9b648' when args.isEmpty =>
      strings.l6EnrollmentStoppedStagedDataAndKeys9b648,
    'l6DeviceKeysOrProtectedRevisionsAre5de96' when args.isEmpty =>
      strings.l6DeviceKeysOrProtectedRevisionsAre5de96,
    'l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35' when args.isEmpty =>
      strings.l6ProtectedRevisionsRolledBackEnrollmentStoppedd7e35,
    'l6DeviceKeyRecordsAreDamagedEnrollment76c86' when args.isEmpty =>
      strings.l6DeviceKeyRecordsAreDamagedEnrollment76c86,
    'l6SecureKeyStorageCouldNotSave6f761' when args.isEmpty =>
      strings.l6SecureKeyStorageCouldNotSave6f761,
    'l6ThisWorkspaceHasAnActiveSync69142' when args.isEmpty =>
      strings.l6ThisWorkspaceHasAnActiveSync69142,
    'l6TheInvitationExpiredOrHasNo6c654' when args.isEmpty =>
      strings.l6TheInvitationExpiredOrHasNo6c654,
    'l6TheRelayStepIsUnconfirmedCheck92aea' when args.isEmpty =>
      strings.l6TheRelayStepIsUnconfirmedCheck92aea,
    'l6EnrollmentIsIncompleteAndHasStopped1c448' when args.isEmpty =>
      strings.l6EnrollmentIsIncompleteAndHasStopped1c448,
    'l6DeviceSyncIsDisabledInThise1b42' when args.isEmpty =>
      strings.l6DeviceSyncIsDisabledInThise1b42,
    'l6ConfirmThatTheJoiningDeviceMay8297c' when args.isEmpty =>
      strings.l6ConfirmThatTheJoiningDeviceMay8297c,
    'l6TheInvitationIsValidFor245be17' when args.isEmpty =>
      strings.l6TheInvitationIsValidFor245be17,
    'l6NoJoinRequestReceivedYet10542' when args.isEmpty =>
      strings.l6NoJoinRequestReceivedYet10542,
    'l6VerifyTheDeviceAndFingerprintBefore5e3e6' when args.isEmpty =>
      strings.l6VerifyTheDeviceAndFingerprintBefore5e3e6,
    'l6ExplicitlyConfirmSharingProviderApiKeysd0cdd' when args.isEmpty =>
      strings.l6ExplicitlyConfirmSharingProviderApiKeysd0cdd,
    'l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8'
        when args.isEmpty =>
      strings.l6ApprovalDeliveredWaitingForInstallationConfirmation8b0b8,
    'l6ApprovalDeliveredWaitingForInstallationConfirmation9b218'
        when args.isEmpty =>
      strings.l6ApprovalDeliveredWaitingForInstallationConfirmation9b218,
    'l6InvitationCanceled35e1b' when args.isEmpty =>
      strings.l6InvitationCanceled35e1b,
    'l6ApprovalDeliveredWaitingForTheNew361e9' when args.isEmpty =>
      strings.l6ApprovalDeliveredWaitingForTheNew361e9,
    'l6TheInvitationOrReviewVersionExpireda53ec' when args.isEmpty =>
      strings.l6TheInvitationOrReviewVersionExpireda53ec,
    'l6TheInvitationWasCanceledLocallyRelay360d8' when args.isEmpty =>
      strings.l6TheInvitationWasCanceledLocallyRelay360d8,
    'l6ApprovalIsSavedButHasNot99508' when args.isEmpty =>
      strings.l6ApprovalIsSavedButHasNot99508,
    'l6BackgroundSyncIsUsingTheRuntime0a21c' when args.isEmpty =>
      strings.l6BackgroundSyncIsUsingTheRuntime0a21c,
    'l6PairingIsIncompleteCheckDeviceUnlock85108' when args.isEmpty =>
      strings.l6PairingIsIncompleteCheckDeviceUnlock85108,
    'l6UnlockThisDeviceSSyncKeysfc734' when args.isEmpty =>
      strings.l6UnlockThisDeviceSSyncKeysfc734,
    'l6LocalSettingsChangedReviewTheUpdated3cd59' when args.isEmpty =>
      strings.l6LocalSettingsChangedReviewTheUpdated3cd59,
    'l6TheConflictChangedReviewTheUpdated936b5' when args.isEmpty =>
      strings.l6TheConflictChangedReviewTheUpdated936b5,
    'l6CouldNotReadDeviceSyncState278eb' when args.isEmpty =>
      strings.l6CouldNotReadDeviceSyncState278eb,
    'l6DeviceSyncIsNotReady3fc30' when args.isEmpty =>
      strings.l6DeviceSyncIsNotReady3fc30,
    'l6DeviceNamesMustContain12561d9b2' when args.isEmpty =>
      strings.l6DeviceNamesMustContain12561d9b2,
    'l6AnotherOperationUpdatedTheStateRefresh39e75' when args.isEmpty =>
      strings.l6AnotherOperationUpdatedTheStateRefresh39e75,
    'l6UnlockDeviceKeysBeforeRenaming10735' when args.isEmpty =>
      strings.l6UnlockDeviceKeysBeforeRenaming10735,
    'l6TheDeviceNameContainsInvalidCharacters438ed' when args.isEmpty =>
      strings.l6TheDeviceNameContainsInvalidCharacters438ed,
    'l6EnterADeviceNameAndConfirm02f54' when args.isEmpty =>
      strings.l6EnterADeviceNameAndConfirm02f54,
    'l6StopEnrollmentBeforePreparingOrUnlocking4420c' when args.isEmpty =>
      strings.l6StopEnrollmentBeforePreparingOrUnlocking4420c,
    'l6LockingIsUnconfirmedTryAgainKeys6bf44' when args.isEmpty =>
      strings.l6LockingIsUnconfirmedTryAgainKeys6bf44,
    'l6DeviceKeysOrProtectedRevisionsAre845e9' when args.isEmpty =>
      strings.l6DeviceKeysOrProtectedRevisionsAre845e9,
    'l6KeyRevisionsRolledBackUnlockingStoppedc58f3' when args.isEmpty =>
      strings.l6KeyRevisionsRolledBackUnlockingStoppedc58f3,
    'l6SystemKeyRecordsAreDamagedUnlocking23c95' when args.isEmpty =>
      strings.l6SystemKeyRecordsAreDamagedUnlocking23c95,
    'l6TheSystemKeyStoreIsUnavailable8d23d' when args.isEmpty =>
      strings.l6TheSystemKeyStoreIsUnavailable8d23d,
    'l6AnotherWindowOrBackgroundTaskIs610c2' when args.isEmpty =>
      strings.l6AnotherWindowOrBackgroundTaskIs610c2,
    'l6BackgroundJobsAreStillRunningPrepare56569' when args.isEmpty =>
      strings.l6BackgroundJobsAreStillRunningPrepare56569,
    'l6TheDeviceSyncIdentityNeedsRecoveryd2a55' when args.isEmpty =>
      strings.l6TheDeviceSyncIdentityNeedsRecoveryd2a55,
    'l6SyncPreparationDidNotFinishAnd1c512' when args.isEmpty =>
      strings.l6SyncPreparationDidNotFinishAnd1c512,
    'l6GeneratingAReplyItContinuesWhence0b9' when args.isEmpty =>
      strings.l6GeneratingAReplyItContinuesWhence0b9,
    'l6CouldNotLoadGenerationStatusReconnect2eabb' when args.isEmpty =>
      strings.l6CouldNotLoadGenerationStatusReconnect2eabb,
    'l6TheStartResultIsUnconfirmedReconnecting3cc5d' when args.isEmpty =>
      strings.l6TheStartResultIsUnconfirmedReconnecting3cc5d,
    'l6TheStopRequestIsUnconfirmedReconnectdaf8d' when args.isEmpty =>
      strings.l6TheStopRequestIsUnconfirmedReconnectdaf8d,
    'l6GenerationStatusIsTemporarilyUnavailableSaved71c5d' when args.isEmpty =>
      strings.l6GenerationStatusIsTemporarilyUnavailableSaved71c5d,
    _ => null,
  };
}
