import '../bridge/api.dart' as api;

abstract interface class SyncPairingApi {
  Future<api.SyncInvitationOfferDto> issue(String relayUrl, bool consent);
  Future<api.SyncInvitationPreviewDto?> inspect(String invitationId);
  Future<api.SyncKeyCommitDto> approve(
    String invitationId,
    String previewToken,
    bool consent,
  );
  Future<void> cancel(String invitationId);
  Future<void> sendApproval(String invitationId);
}

class FrbSyncPairingApi implements SyncPairingApi {
  @override
  Future<api.SyncInvitationOfferDto> issue(String relayUrl, bool consent) =>
      api.createOwnerSyncInvitation(
        relayUrl: relayUrl,
        acknowledgeProviderKeySharing: consent,
      );

  @override
  Future<api.SyncInvitationPreviewDto?> inspect(String invitationId) =>
      api.inspectOwnerSyncInvitation(invitationId: invitationId);

  @override
  Future<api.SyncKeyCommitDto> approve(
    String invitationId,
    String previewToken,
    bool consent,
  ) => api.approveOwnerSyncInvitation(
    invitationId: invitationId,
    previewToken: previewToken,
    acknowledgeProviderKeySharing: consent,
  );

  @override
  Future<void> cancel(String invitationId) =>
      api.cancelOwnerSyncInvitation(invitationId: invitationId);

  @override
  Future<void> sendApproval(String invitationId) =>
      api.sendOwnerSyncApproval(invitationId: invitationId);
}
