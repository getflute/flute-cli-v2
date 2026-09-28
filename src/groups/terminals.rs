//! `terminals`: list and status.
//!
//! Two reads and no writes, which makes this the one group whose whole surface
//! is query parameters. Both operations report a `terminalStatus`, and the two
//! declared enums for it do not agree: the *query* parameter declares `Ready`,
//! `Busy`, `Offline` and both *responses* declare `Active`, `Busy`, `Offline`.
//! The CLI sends what the parameter declares and prints what the API answers,
//! so the disagreement stays visible instead of being papered over.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common::{self, PaginationArgs};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;

/// The `terminalStatus` **query** enum, which is not the response enum.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum TerminalStatusFilter {
    Ready,
    Busy,
    Offline,
}

impl TerminalStatusFilter {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Ready => "Ready",
            Self::Busy => "Busy",
            Self::Offline => "Offline",
        }
    }
}

/// `Standalone` or `SemiIntegrated`. Only a semi-integrated terminal can take
/// a POS transaction the CLI created.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum TerminalMode {
    Standalone,
    SemiIntegrated,
}

impl TerminalMode {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Standalone => "Standalone",
            Self::SemiIntegrated => "SemiIntegrated",
        }
    }
}

/// `Online` or `Offline`: whether the device is reachable now.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ConnectionStatus {
    Online,
    Offline,
}

impl ConnectionStatus {
    pub fn wire(self) -> &'static str {
        match self {
            Self::Online => "Online",
            Self::Offline => "Offline",
        }
    }
}

#[allow(clippy::large_enum_variant)]
#[derive(clap::Subcommand, Debug)]
pub enum TerminalsCommand {
    /// List terminals (GET /v2/terminals).
    List(ListTerminalsArgs),
    /// Retrieve terminal status by ID
    /// (GET /v2/terminals/{terminalId}/status).
    ///
    /// Reports the terminal's live state: connection, battery and printer.
    Status {
        /// Terminal UUID (positional).
        terminal_id: String,
    },
}

#[derive(clap::Args, Debug, Default)]
pub struct ListTerminalsArgs {
    #[command(flatten)]
    pub pagination: PaginationArgs,
    /// Sort results by this field name.
    #[arg(long, id = "terminal_sort_by", value_name = "SORT_BY")]
    pub sort_by: Option<String>,
    /// Sort ascending. With neither `--asc` nor `--desc`, the server's default
    /// order applies.
    #[arg(long, id = "terminal_asc", conflicts_with = "terminal_desc")]
    pub asc: bool,
    /// Sort descending.
    #[arg(long, id = "terminal_desc")]
    pub desc: bool,
    /// Filter by readiness: `ready`, `busy` or `offline`.
    #[arg(
        long = "status",
        value_enum,
        id = "terminal_status",
        value_name = "TERMINAL_STATUS"
    )]
    pub terminal_status: Option<TerminalStatusFilter>,
    /// Filter by mode: `standalone` or `semi-integrated`.
    #[arg(long = "mode", value_enum, value_name = "TERMINAL_MODE")]
    pub terminal_mode: Option<TerminalMode>,
    /// Filter by line state: `online` or `offline`.
    #[arg(long = "connection", value_enum, value_name = "CONNECTION_STATUS")]
    pub connection_status: Option<ConnectionStatus>,
    /// Filter by serial number.
    #[arg(long)]
    pub serial_number: Option<String>,
    /// Server-side text search.
    #[arg(long, id = "terminal_search", value_name = "SEARCH")]
    pub search: Option<String>,
}

/// The `GET /v2/terminals` query, omitting every absent flag.
pub fn build_list_terminals_query(args: &ListTerminalsArgs) -> Result<Vec<(&'static str, String)>> {
    args.pagination.validate()?;
    let mut query = args.pagination.query();
    // Both directions are sent explicitly: `sortOrder` declares a default of
    // "asc", and the sibling lists answer newest first when it is omitted.
    if args.asc {
        query.push(("sortOrder", "asc".into()));
    } else if args.desc {
        query.push(("sortOrder", "desc".into()));
    }
    if let Some(v) = args.terminal_status {
        query.push(("terminalStatus", v.wire().into()));
    }
    if let Some(v) = args.terminal_mode {
        query.push(("terminalMode", v.wire().into()));
    }
    if let Some(v) = args.connection_status {
        query.push(("connectionStatus", v.wire().into()));
    }
    let mut put = |key: &'static str, value: &Option<String>| {
        if let Some(v) = value.as_ref().filter(|s| !s.is_empty()) {
            query.push((key, v.clone()));
        }
    };
    put("sortBy", &args.sort_by);
    put("serialNumber", &args.serial_number);
    put("search", &args.search);
    Ok(query)
}

/// What a terminal is worth saying: identity, then what it is, then how it is
/// doing.
pub static TERMINAL: Resource = Resource {
    object: "terminal",
    object_list: "terminal_list",
    id: "/terminalId",
    detail: &[
        "/terminalId",
        "/serialNumber",
        "/terminalManufacturer",
        "/terminalModel",
        "/terminalMode",
        "/terminalStatus",
        "/connectionStatus",
        "/lastSeenOn",
        "/merchantId",
        "/merchantCompanyName",
    ],
    // A caller choosing a terminal to charge on needs the serial number they
    // can read off the case, the mode that decides whether it can take the
    // transaction at all, whether it is reachable, and when it last was.
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/terminalId"),
        },
        Column {
            header: "SERIAL",
            width: 16,
            cell: Cell::Path("/serialNumber"),
        },
        Column {
            header: "MODEL",
            width: 20,
            cell: Cell::Path("/terminalModel"),
        },
        Column {
            header: "MODE",
            width: 20,
            cell: Cell::Path("/terminalMode"),
        },
        Column {
            header: "CONNECTION",
            width: 12,
            cell: Cell::Path("/connectionStatus"),
        },
        Column {
            header: "LAST SEEN",
            width: 24,
            cell: Cell::Path("/lastSeenOn"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

/// A terminal's live state is a different resource from the terminal, and v1
/// gave it its own envelope name. The name is part of the output contract, so
/// it is kept.
pub static TERMINAL_STATUS: Resource = Resource {
    object: "terminal_status",
    object_list: "terminal_statuses",
    id: "/terminalId",
    detail: &[
        "/terminalId",
        "/terminalStatus",
        "/connectionStatus",
        "/connectionType",
        "/wifiConnectionStrength",
        "/mobileConnectionStrength",
        "/batteryLevel",
        "/printerStatus",
        "/debitPinKey",
        "/terminalAppVersion",
        "/lastSeenOn",
        "/merchantId",
    ],
    // The endpoint answers one terminal, so there is no collection to name
    // columns for; the detail view is the whole point of the command.
    columns: &[
        Column {
            header: "ID",
            width: 36,
            cell: Cell::Path("/terminalId"),
        },
        Column {
            header: "STATUS",
            width: 8,
            cell: Cell::Path("/terminalStatus"),
        },
        Column {
            header: "LINE",
            width: 8,
            cell: Cell::Path("/connectionStatus"),
        },
    ],
    amounts: &[],
    yes_no: &[],
};

pub async fn dispatch(ctx: &Ctx, command: TerminalsCommand) -> Result<()> {
    match command {
        TerminalsCommand::List(args) => {
            let query = build_list_terminals_query(&args)?;
            if args.pagination.all {
                let (items, correlation_id) =
                    common::fetch_all(&ctx.api, "/v2/terminals", &query, args.pagination.page_size)
                        .await?;
                render::page(ctx, &TERMINAL, &items, None, correlation_id)
            } else {
                let resp = ctx
                    .api
                    .request(Method::GET, "/v2/terminals", &query, None)
                    .await?;
                let body = common::body_of(resp.body)?;
                render::page(
                    ctx,
                    &TERMINAL,
                    &common::items_of(&body)?,
                    body.get("pageInfo").cloned(),
                    resp.correlation_id,
                )
            }
        }
        TerminalsCommand::Status { terminal_id } => {
            let resp = ctx
                .api
                .request(
                    Method::GET,
                    ApiPath::from("/v2/terminals")
                        .id(&terminal_id)?
                        .seg("status"),
                    &[],
                    None,
                )
                .await?;
            render::one(
                ctx,
                &TERMINAL_STATUS,
                &common::body_of(resp.body)?,
                resp.correlation_id,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list_args() -> ListTerminalsArgs {
        ListTerminalsArgs::default()
    }

    /// No flags means no query: the server's declared defaults govern.
    #[test]
    fn an_unfiltered_terminal_list_sends_nothing() {
        assert!(build_list_terminals_query(&list_args()).unwrap().is_empty());
    }

    #[test]
    fn every_terminal_filter_reaches_the_query_under_its_wire_name() {
        let args = ListTerminalsArgs {
            pagination: PaginationArgs {
                page_index: Some(1),
                page_size: Some(5),
                all: false,
            },
            sort_by: Some("terminalManufacturer".into()),
            asc: false,
            desc: true,
            terminal_status: Some(TerminalStatusFilter::Ready),
            terminal_mode: Some(TerminalMode::SemiIntegrated),
            connection_status: Some(ConnectionStatus::Online),
            serial_number: Some("SN100001".into()),
            search: Some("front counter".into()),
        };
        let q = build_list_terminals_query(&args).unwrap();
        let names: Vec<&str> = q.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            names,
            [
                "pageIndex",
                "pageSize",
                "sortOrder",
                "terminalStatus",
                "terminalMode",
                "connectionStatus",
                "sortBy",
                "serialNumber",
                "search"
            ]
        );
        let value = |k: &str| q.iter().find(|(n, _)| *n == k).unwrap().1.clone();
        assert_eq!(value("terminalStatus"), "Ready");
        assert_eq!(value("terminalMode"), "SemiIntegrated");
        assert_eq!(value("connectionStatus"), "Online");
        assert_eq!(value("sortOrder"), "desc");
    }

    /// The filter takes the enum the **query parameter** declares. `Active`
    /// is a response value and is not offered here, because sending it would
    /// be a filter the parameter does not document.
    #[test]
    fn the_status_filter_uses_the_query_enum_not_the_response_enum() {
        assert_eq!(TerminalStatusFilter::Ready.wire(), "Ready");
        for variant in [
            TerminalStatusFilter::Ready,
            TerminalStatusFilter::Busy,
            TerminalStatusFilter::Offline,
        ] {
            assert_ne!(variant.wire(), "Active");
        }
    }

    /// Each direction is sent explicitly, and neither flag sends nothing.
    #[test]
    fn terminal_list_sends_the_sort_order_it_is_asked_for() {
        let order = |asc, desc| {
            let mut args = list_args();
            args.asc = asc;
            args.desc = desc;
            build_list_terminals_query(&args)
                .unwrap()
                .into_iter()
                .find(|(k, _)| *k == "sortOrder")
                .map(|(_, v)| v)
        };
        assert_eq!(order(true, false).as_deref(), Some("asc"));
        assert_eq!(order(false, true).as_deref(), Some("desc"));
        assert_eq!(order(false, false), None);
    }

    /// An empty filter string is not a filter.
    #[test]
    fn an_empty_terminal_filter_is_treated_as_absent() {
        let mut args = list_args();
        args.search = Some(String::new());
        assert!(build_list_terminals_query(&args).unwrap().is_empty());
    }

    /// The shared bound, reached through the flattened struct rather than
    /// restated here.
    #[test]
    fn terminal_list_inherits_the_shared_page_size_bound() {
        let mut args = list_args();
        args.pagination.page_size = Some(101);
        assert!(build_list_terminals_query(&args).is_err());
    }
}
