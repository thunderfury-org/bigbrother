use crate::{
    application::{
        delete_media::DeleteMediaService,
        file_index::FileIndexService,
        import::{ParseService, TransferWorkflow, identify::MediaIdentifyService},
        media_source_observation::ProcessObservationService,
        resolve_download_url::ResolveDownloadUrlService,
        subscription::manage::ManageSubscriptionsService,
    },
    infrastructure::{
        client::{pan115, pan123, pan189},
        event::publisher::EventBusPublisher,
        share::resolver::ShareResolverService,
    },
};

pub type ShareResolverRuntimeService =
    ShareResolverService<pan123::Client, pan189::Client, pan115::Client>;
pub type ImportService = TransferWorkflow;
pub type IdentifyService = MediaIdentifyService;
pub type NotifyService = EventBusPublisher;
pub type MediaDownloadUrlService = ResolveDownloadUrlService;
pub type DeleteMediaServiceRuntime = DeleteMediaService;
pub type FileIndexRuntimeService = FileIndexService;
pub type ParseRuntimeService = ParseService;
pub type SubscriptionService = ManageSubscriptionsService;
pub type ObservationProcessor = ProcessObservationService;
