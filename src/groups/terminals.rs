//! `terminals`: list and status.
//!
//! Two reads and no writes, which makes this the one group whose whole surface
//! is query parameters. Both operations report a `terminalStatus` of `Ready`,
//! `Busy` or `Offline`, the same three values the query parameter filters on.
//! The published response schemas declare `Active` in place of `Ready`; the
//! API answers `Ready`, and refuses `Active` as a filter value.

use crate::Ctx;
use crate::api::ApiPath;
use crate::cli::common::{self, PaginationArgs};
use crate::cli::render::{self, Cell, Column, Resource};
use anyhow::Result;
use reqwest::Method;

/// The `terminalStatus` filter: the same values a response reports.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum TerminalStatusFilter {
    Ready,
    Busy,
    Offline,
}

/// `Standalone` or `SemiIntegrated`. Only a semi-integrated terminal can take
/// a POS transaction the CLI created.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum TerminalMode {
    Standalone,
    SemiIntegrated,
}

/// `Online` or `Offline`: whether the device is reachable now.
#[derive(Copy, Clone, Debug, PartialEq, Eq, clap::ValueEnum, serde::Serialize)]
pub enum ConnectionStatus {
    Online,
    Offline,
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
    query.extend(common::sort_order(args.asc, args.desc));
    if let Some(v) = args.terminal_status {
        query.push(("terminalStatus", common::wire(v)));
    }
    if let Some(v) = args.terminal_mode {
        query.push(("terminalMode", common::wire(v)));
    }
    if let Some(v) = args.connection_status {
        query.push(("connectionStatus", common::wire(v)));
    }
    common::push_str(&mut query, "sortBy", &args.sort_by);
    common::push_id(
        &mut query,
        "--serial-number",
        "serialNumber",
        &args.serial_number,
    )?;
    common::push_str(&mut query, "search", &args.search);
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
        "/terminalModel",
        "/terminalStatus",
        "/connectionStatus",
        "/lastSeenOn",
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

/// A terminal's live state is a different resource from the terminal, so it
/// has its own envelope name, which is part of the output contract.
pub static TERMINAL_STATUS: Resource = Resource {
    object: "terminal_status",
    object_list: "terminal_statuses",
    id: "/terminalId",
    detail: &[
        "/terminalId",
        "/terminalStatus",
        "/connectionStatus",
        "/connectionType",
        "/batteryLevel",
        "/wifiConnectionStrength",
        "/printerStatus",
        "/lastSeenOn",
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
            common::list(ctx, &TERMINAL, "/v2/terminals", &query, &args.pagination).await
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
        assert_eq!(serde_json::json!(TerminalStatusFilter::Ready), "Ready");
        for variant in [
            TerminalStatusFilter::Ready,
            TerminalStatusFilter::Busy,
            TerminalStatusFilter::Offline,
        ] {
            assert_ne!(serde_json::json!(variant), "Active");
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

    /// An empty search is no search. An empty serial number names no
    /// terminal, and dropping it would answer with every terminal.
    #[test]
    fn an_empty_search_is_absent_and_an_empty_serial_number_is_refused() {
        let mut args = list_args();
        args.search = Some(String::new());
        assert!(build_list_terminals_query(&args).unwrap().is_empty());

        let mut args = list_args();
        args.serial_number = Some(" ".into());
        let err = build_list_terminals_query(&args).unwrap_err().to_string();
        assert!(err.contains("--serial-number needs a value"), "{err}");
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
