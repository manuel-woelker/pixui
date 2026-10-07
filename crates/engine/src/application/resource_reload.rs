//! Internal handoff between resource preparation and application mutation.
use super::{
    application_handle::ApplicationHandle,
    dispatch::{ApplicationCommand, ApplicationReply},
};
use crate::resources::{
    image_loader::ImageLoader,
    reload::{
        shared::Shared,
        target::{CatalogTarget, Prepared},
    },
};
use crossbeam_channel::{TrySendError, bounded};
use pixui_base::{PixuiResult, pixui_error};
use std::sync::Arc;

impl ApplicationHandle {
    pub(crate) fn reload_image_loader(&self) -> PixuiResult<ImageLoader> {
        self.inspect(|app| {
            app.image_service
                .borrow()
                .loader()
                .cloned()
                .ok_or_else(|| pixui_error!("no application image loader configured"))
        })
    }
    pub(crate) fn attach_resource_reload(
        &self,
        shared: Arc<Shared>,
        loader: Option<ImageLoader>,
        catalogs: Vec<CatalogTarget>,
    ) -> PixuiResult<()> {
        self.request(move |app| {
            if app
                .resource_reload
                .as_ref()
                .and_then(std::sync::Weak::upgrade)
                .is_some_and(|state| state.active())
            {
                return Err(pixui_error!(
                    "application already has a resource reload session"
                ));
            }
            for catalog in catalogs {
                app.translations.validate_language(catalog.language)?;
            }
            if let Some(loader) = loader {
                let service = app.image_service.get_mut();
                if !service
                    .loader()
                    .is_some_and(|current| current.same_configuration(&loader))
                {
                    return Err(pixui_error!("image loader changed during reload startup"));
                }
                service.watch(shared.clone())?;
            }
            app.resource_reload = Some(Arc::downgrade(&shared));
            Ok(())
        })?
        .wait()
    }
    pub(crate) fn try_resource_update(
        &self,
        update: Box<Prepared>,
    ) -> Result<ApplicationReply<bool>, TrySendError<Box<Prepared>>> {
        let (reply, receiver) = bounded(1);
        self.sender
            .try_send(ApplicationCommand::Resource { update, reply })
            .map_err(|error| match error {
                TrySendError::Full(ApplicationCommand::Resource { update, .. }) => {
                    TrySendError::Full(update)
                }
                TrySendError::Disconnected(ApplicationCommand::Resource { update, .. }) => {
                    TrySendError::Disconnected(update)
                }
                _ => unreachable!("resource command sent here"),
            })?;
        Ok(ApplicationReply::new(receiver))
    }
}
