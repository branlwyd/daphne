// Copyright (c) 2022 Cloudflare, Inc. All rights reserved.
// SPDX-License-Identifier: BSD-3-Clause

//! Constants used in the DAP protocol.

use crate::{DapSender, DapVersion};

// Media types for HTTP requests.
const DRAFT02_MEDIA_TYPE_AGG_CONT_REQ: &str = "application/dap-aggregate-continue-req";
const DRAFT02_MEDIA_TYPE_AGG_CONT_RESP: &str = "application/dap-aggregate-continue-resp";
const DRAFT02_MEDIA_TYPE_AGG_INIT_REQ: &str = "application/dap-aggregate-initialize-req";
const DRAFT02_MEDIA_TYPE_AGG_INIT_RESP: &str = "application/dap-aggregate-initialize-resp";
const DRAFT02_MEDIA_TYPE_AGG_SHARE_RESP: &str = "application/dap-aggregate-share-resp";
const DRAFT02_MEDIA_TYPE_COLLECT_RESP: &str = "application/dap-collect-resp";
const DRAFT02_MEDIA_TYPE_HPKE_CONFIG: &str = "application/dap-hpke-config";
const MEDIA_TYPE_AGG_JOB_CONT_REQ: &str = "application/dap-aggregation-job-continue-req";
const MEDIA_TYPE_AGG_JOB_INIT_REQ: &str = "application/dap-aggregation-job-init-req";
const MEDIA_TYPE_AGG_JOB_RESP: &str = "application/dap-aggregation-job-resp";
const MEDIA_TYPE_AGG_SHARE_REQ: &str = "application/dap-aggregate-share-req";
const MEDIA_TYPE_AGG_SHARE: &str = "application/dap-aggregate-share";
const MEDIA_TYPE_COLLECTION: &str = "application/dap-collection";
const MEDIA_TYPE_COLLECT_REQ: &str = "application/dap-collect-req";
const MEDIA_TYPE_HPKE_CONFIG_LIST: &str = "application/dap-hpke-config-list";
const MEDIA_TYPE_REPORT: &str = "application/dap-report";

/// Media type for each DAP request. This is included in the "content-type" HTTP header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DapMediaType {
    AggregationJobInitReq,
    AggregationJobResp,
    AggregationJobContinueReq,
    /// draft02 compatibility: the latest draft doesn't define a separate media type for initialize
    /// and continue responses, but draft02 does.
    Draft02AggregateContinueResp,
    AggregateShareReq,
    AggregateShare,
    CollectReq,
    Collection,
    HpkeConfigList,
    Report,
}

impl DapMediaType {
    /// Return the sender that would send a DAP request or response with the given media type (or
    /// none if the sender can't be determined).
    pub fn sender(&self) -> DapSender {
        match self {
            Self::AggregationJobInitReq
            | Self::AggregationJobContinueReq
            | Self::AggregateShareReq
            | Self::Collection
            | Self::HpkeConfigList => DapSender::Leader,
            Self::AggregationJobResp
            | Self::Draft02AggregateContinueResp
            | Self::AggregateShare => DapSender::Helper,
            Self::Report => DapSender::Client,
            Self::CollectReq => DapSender::Collector,
        }
    }

    /// Parse the media type from the content-type HTTP header.
    pub fn from_str_for_version(version: DapVersion, content_type: &str) -> Option<Self> {
        let (content_type, _) = content_type.split_once(';').unwrap_or((content_type, ""));
        let media_type = match (version, content_type) {
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_AGG_CONT_REQ)
            | (DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_AGG_JOB_CONT_REQ) => {
                Self::AggregationJobContinueReq
            }
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_AGG_CONT_RESP) => {
                Self::Draft02AggregateContinueResp
            }
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_AGG_INIT_REQ)
            | (DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_AGG_JOB_INIT_REQ) => {
                Self::AggregationJobInitReq
            }
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_AGG_INIT_RESP)
            | (DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_AGG_JOB_RESP) => {
                Self::AggregationJobResp
            }
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_AGG_SHARE_RESP)
            | (DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_AGG_SHARE) => {
                Self::AggregateShare
            }
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_COLLECT_RESP)
            | (DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_COLLECTION) => Self::Collection,
            (DapVersion::Draft02, DRAFT02_MEDIA_TYPE_HPKE_CONFIG)
            | (DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_HPKE_CONFIG_LIST) => {
                Self::HpkeConfigList
            }
            (
                DapVersion::Draft02 | DapVersion::Draft09 | DapVersion::Latest,
                MEDIA_TYPE_AGG_SHARE_REQ,
            ) => Self::AggregateShareReq,
            (
                DapVersion::Draft02 | DapVersion::Draft09 | DapVersion::Latest,
                MEDIA_TYPE_COLLECT_REQ,
            ) => Self::CollectReq,
            (DapVersion::Draft02 | DapVersion::Draft09 | DapVersion::Latest, MEDIA_TYPE_REPORT) => {
                Self::Report
            }
            (_, _) => return None,
        };
        Some(media_type)
    }

    /// Get the content-type representation of the media type.
    pub fn as_str_for_version(&self, version: DapVersion) -> Option<&'static str> {
        match (version, self) {
            (DapVersion::Draft02, Self::AggregationJobInitReq) => {
                Some(DRAFT02_MEDIA_TYPE_AGG_INIT_REQ)
            }
            (DapVersion::Draft09 | DapVersion::Latest, Self::AggregationJobInitReq) => {
                Some(MEDIA_TYPE_AGG_JOB_INIT_REQ)
            }
            (DapVersion::Draft02, Self::AggregationJobResp) => {
                Some(DRAFT02_MEDIA_TYPE_AGG_INIT_RESP)
            }
            (DapVersion::Draft09 | DapVersion::Latest, Self::AggregationJobResp) => {
                Some(MEDIA_TYPE_AGG_JOB_RESP)
            }
            (DapVersion::Draft02, Self::AggregationJobContinueReq) => {
                Some(DRAFT02_MEDIA_TYPE_AGG_CONT_REQ)
            }
            (DapVersion::Draft09 | DapVersion::Latest, Self::AggregationJobContinueReq) => {
                Some(MEDIA_TYPE_AGG_JOB_CONT_REQ)
            }
            (DapVersion::Draft02, Self::Draft02AggregateContinueResp) => {
                Some(DRAFT02_MEDIA_TYPE_AGG_CONT_RESP)
            }
            (
                DapVersion::Draft02 | DapVersion::Draft09 | DapVersion::Latest,
                Self::AggregateShareReq,
            ) => Some(MEDIA_TYPE_AGG_SHARE_REQ),
            (DapVersion::Draft02, Self::AggregateShare) => Some(DRAFT02_MEDIA_TYPE_AGG_SHARE_RESP),
            (DapVersion::Draft09 | DapVersion::Latest, Self::AggregateShare) => {
                Some(MEDIA_TYPE_AGG_SHARE)
            }
            (DapVersion::Draft02 | DapVersion::Draft09 | DapVersion::Latest, Self::CollectReq) => {
                Some(MEDIA_TYPE_COLLECT_REQ)
            }
            (DapVersion::Draft02, Self::Collection) => Some(DRAFT02_MEDIA_TYPE_COLLECT_RESP),
            (DapVersion::Draft09 | DapVersion::Latest, Self::Collection) => {
                Some(MEDIA_TYPE_COLLECTION)
            }
            (DapVersion::Draft02, Self::HpkeConfigList) => Some(DRAFT02_MEDIA_TYPE_HPKE_CONFIG),
            (DapVersion::Draft09 | DapVersion::Latest, Self::HpkeConfigList) => {
                Some(MEDIA_TYPE_HPKE_CONFIG_LIST)
            }
            (DapVersion::Draft02 | DapVersion::Draft09 | DapVersion::Latest, Self::Report) => {
                Some(MEDIA_TYPE_REPORT)
            }
            (_, Self::Draft02AggregateContinueResp) => None,
        }
    }

    /// draft02 compatibility: Construct the media type for the response to an
    /// `AggregatecontinueResp`. This various depending upon the version used.
    pub(crate) fn agg_job_cont_resp_for_version(version: DapVersion) -> Self {
        match version {
            DapVersion::Draft02 => Self::Draft02AggregateContinueResp,
            DapVersion::Draft09 | DapVersion::Latest => Self::AggregationJobResp,
        }
    }
}

#[cfg(test)]
mod test {
    use super::DapMediaType;
    use crate::DapVersion;

    #[test]
    fn from_str_for_version() {
        // draft02, Section 8.1
        assert_eq!(
            DapMediaType::from_str_for_version(DapVersion::Draft02, "application/dap-hpke-config"),
            Some(DapMediaType::HpkeConfigList)
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft02,
                "application/dap-aggregate-initialize-req"
            ),
            Some(DapMediaType::AggregationJobInitReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft02,
                "application/dap-aggregate-initialize-resp"
            ),
            Some(DapMediaType::AggregationJobResp),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft02,
                "application/dap-aggregate-continue-req"
            ),
            Some(DapMediaType::AggregationJobContinueReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft02,
                "application/dap-aggregate-continue-resp"
            ),
            Some(DapMediaType::Draft02AggregateContinueResp),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft02,
                "application/dap-aggregate-share-req"
            ),
            Some(DapMediaType::AggregateShareReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft02,
                "application/dap-aggregate-share-resp"
            ),
            Some(DapMediaType::AggregateShare),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(DapVersion::Draft02, "application/dap-collect-req"),
            Some(DapMediaType::CollectReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(DapVersion::Draft02, "application/dap-collect-resp"),
            Some(DapMediaType::Collection),
        );

        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-hpke-config-list",
            ),
            Some(DapMediaType::HpkeConfigList)
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-aggregation-job-init-req"
            ),
            Some(DapMediaType::AggregationJobInitReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-aggregation-job-resp"
            ),
            Some(DapMediaType::AggregationJobResp),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-aggregation-job-continue-req"
            ),
            Some(DapMediaType::AggregationJobContinueReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-aggregate-share-req"
            ),
            Some(DapMediaType::AggregateShareReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-aggregate-share"
            ),
            Some(DapMediaType::AggregateShare),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(DapVersion::Draft09, "application/dap-collect-req"),
            Some(DapMediaType::CollectReq),
        );
        assert_eq!(
            DapMediaType::from_str_for_version(DapVersion::Draft09, "application/dap-collection"),
            Some(DapMediaType::Collection),
        );

        // Invalid media type
        assert_eq!(
            DapMediaType::from_str_for_version(DapVersion::Draft09, "blah-blah-blah"),
            None,
        );
    }

    #[test]
    fn round_trip() {
        for (version, media_type) in [
            (DapVersion::Draft02, DapMediaType::AggregationJobInitReq),
            (DapVersion::Draft09, DapMediaType::AggregationJobInitReq),
            (DapVersion::Draft02, DapMediaType::AggregationJobResp),
            (DapVersion::Draft09, DapMediaType::AggregationJobResp),
            (DapVersion::Draft02, DapMediaType::AggregationJobContinueReq),
            (DapVersion::Draft09, DapMediaType::AggregationJobContinueReq),
            (
                DapVersion::Draft02,
                DapMediaType::Draft02AggregateContinueResp,
            ),
            (DapVersion::Draft02, DapMediaType::AggregateShareReq),
            (DapVersion::Draft09, DapMediaType::AggregateShareReq),
            (DapVersion::Draft02, DapMediaType::AggregateShare),
            (DapVersion::Draft09, DapMediaType::AggregateShare),
            (DapVersion::Draft02, DapMediaType::CollectReq),
            (DapVersion::Draft09, DapMediaType::CollectReq),
            (DapVersion::Draft02, DapMediaType::Collection),
            (DapVersion::Draft09, DapMediaType::Collection),
            (DapVersion::Draft02, DapMediaType::HpkeConfigList),
            (DapVersion::Draft09, DapMediaType::HpkeConfigList),
            (DapVersion::Draft02, DapMediaType::Report),
            (DapVersion::Draft09, DapMediaType::Report),
        ] {
            assert_eq!(
                media_type
                    .as_str_for_version(version)
                    .and_then(|mime| DapMediaType::from_str_for_version(version, mime)),
                Some(media_type),
                "round trip test failed for {version:?} and {media_type:?}"
            );
        }
    }

    // Issue #269: Ensure the media type included with the AggregateContinueResp in draft02 is not
    // overwritten by the media type for AggregationJobResp.
    #[test]
    fn media_type_for_agg_cont_req() {
        assert_eq!(
            DapMediaType::Draft02AggregateContinueResp,
            DapMediaType::agg_job_cont_resp_for_version(DapVersion::Draft02)
        );

        assert_eq!(
            DapMediaType::AggregationJobResp,
            DapMediaType::agg_job_cont_resp_for_version(DapVersion::Draft09)
        );
    }

    #[test]
    fn media_type_parsing_ignores_content_type_paramters() {
        assert_eq!(
            DapMediaType::from_str_for_version(
                DapVersion::Draft09,
                "application/dap-aggregation-job-init-req;version=09",
            ),
            Some(DapMediaType::AggregationJobInitReq),
        );
    }
}
