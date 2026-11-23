import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./instances.css";
import {
  Activity,
  AlertCircle,
  ArrowRight,
  Blocks,
  CheckCircle2,
  Cpu,
  Download,
  FolderOpen,
  Gamepad2,
  Gauge,
  Layers3,
  Monitor,
  Package,
  Play,
  Puzzle,
  Square,
  Terminal,
  RefreshCw,
  ScanSearch,
  Settings2,
  ShieldCheck,
  TerminalSquare,
  UserRound,
  Zap
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

type Page = "overview" | "instances" | "accounts" | "modpacks" | "mods" | "downloads" | "performance" | "diagnostics" | "settings";

type RuntimeStatus = {
  platform: string;
  architecture: string;
  java_available: boolean;
  java_version: string | null;
  minecraft_directory: string | null;
};


type MinecraftVersion = {
  id: string;
  versionType: string;
  releaseTime: string;
};
type VersionCatalog = {
  latest: { release: string; snapshot: string };
  versions: MinecraftVersion[];
};
type InstalledInstance = {
  id: string;
  versionType: string;
  installedAtUnix: number;
  totalBytes: number;
  files: number;
  instancePath: string;
  clientSha1: string;
  assetIndex: string;
};
type InstallProgress = {
  phase: string;
  completedFiles: number;
  totalFiles: number;
  currentFile: string;
  transferredBytes: number;
  totalBytes: number | null;
  attempt: number;
};

type LaunchSession = {
  instanceId: string;
  pid: number;
  startedAtUnix: number;
  state: string;
};
type ProcessLog = { instanceId: string; stream: string; line: string };
type ProcessState = { instanceId: string; state: string; pid: number | null; exitCode: number | null; message: string | null };

type PerformanceSnapshot = {
  uptimeMs: number;
  activeOperations: number;
  peakOperations: number;
  completedOperations: number;
  totalOperationTimeMs: number;
  downloadedBytes: number;
  reusedBytes: number;
  cacheHits: number;
  cacheMisses: number;
  verificationFailures: number;
  cacheHitRateBasisPoints: number;
};

type DiagnosticCheck = {
  id: string;
  category: string;
  severity: string;
  status: string;
  title: string;
  details: string;
  evidence: string[];
  recommendedAction: string | null;
};
type CrashDiagnosis = {
  code: string;
  severity: string;
  confidence: string;
  title: string;
  likelyCause: string;
  evidence: string[];
  recommendedAction: string;
};
type DiagnosticReport = {
  schemaVersion: number;
  reportId: string;
  generatedAtUnix: number;
  trigger: string;
  mode: string;
  summary: { overallStatus: string; checksTotal: number; passed: number; warnings: number; failures: number; information: number };
  system: {
    operatingSystem: string;
    architecture: string;
    osVersion: string | null;
    osBuild: string | null;
    cpuModel: string | null;
    logicalProcessors: number;
    totalMemoryBytes: number | null;
    gpus: { name: string; driverVersion: string | null; adapterMemoryBytes: number | null }[];
    hardwareQueryStatus: string;
  };
  instances: {
    id: string;
    versionType: string | null;
    clientStatus: string;
    clientExpectedSha1: string | null;
    clientActualSha1: string | null;
    javaRequiredMajor: number | null;
    javaCandidates: { executable: string; version: string | null; majorVersion: number | null; compatible: boolean | null; source: string }[];
    libraryCount: number;
    missingLibraries: number;
    invalidLibraries: number;
    assetCount: number;
    missingAssets: number;
    invalidAssets: number;
    nativeFileCount: number;
    latestLogExcerpt: string[];
    crashReportExcerpt: string[];
    crashArtifactNames: string[];
    rootCauseHypotheses: CrashDiagnosis[];
  }[];
  network: { name: string; host: string; statusCode: number | null; latencyMs: number | null; reachable: boolean; detail: string }[];
  rootCauseHypotheses: CrashDiagnosis[];
  checks: DiagnosticCheck[];
  recentProcessLogs: { timestampUnix: number; stream: string; line: string }[];
  reportPath: string;
  privacy: string[];
};

const PAGE_INFO: Record<Page, { eyebrow: string; title: string; description: string }> = {
  overview: {
    eyebrow: "YOUR MINECRAFT WORKSPACE",
    title: "Good to have you here.",
    description: "Everything you need for your next adventure, in one place."
  },
  instances: {
    eyebrow: "YOUR GAME LIBRARY",
    title: "Versions",
    description: "Install official Minecraft releases and manage your local game profiles."
  },
  accounts: {
    eyebrow: "PLAYER PROFILE",
    title: "Accounts",
    description: "Manage the local offline profile used when launching Minecraft."
  },
  modpacks: {
    eyebrow: "DISCOVER SOMETHING NEW",
    title: "Modpacks",
    description: "Find a new way to play. Provider-powered discovery is on the roadmap."
  },
  mods: {
    eyebrow: "MAKE IT YOURS",
    title: "Mods",
    description: "Manage compatible mods for each instance without mixing configurations."
  },
  downloads: {
    eyebrow: "TRANSFER CENTER",
    title: "Downloads",
    description: "Track installation tasks, updates and their verification status."
  },
  performance: {
    eyebrow: "MEASURED, NOT GUESSED",
    title: "Performance",
    description: "Inspect native workload metrics and the optimization engine's measured activity."
  },
  diagnostics: {
    eyebrow: "AUTOMATIC TROUBLESHOOTING",
    title: "Diagnostics",
    description: "Inspect hardware, Java, Minecraft files, integrity checks and crash evidence in a local, shareable report."
  },
  settings: {
    eyebrow: "PREFERENCES",
    title: "Settings",
    description: "Configure the launcher, runtime and your local environment."
  }
};

function useDesktopRuntime() {
  const [runtime, setRuntime] = useState<RuntimeStatus | null>(null);
  const [checking, setChecking] = useState(true);
  const isDesktop =
    typeof window !== "undefined" &&
    Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__);

  const refresh = useCallback(async () => {
    if (!isDesktop) {
      setChecking(false);
      return;
    }

    setChecking(true);
    try {
      const result = await invoke<RuntimeStatus>("get_runtime_status");
      setRuntime(result);
    } catch (error) {
      console.error("Unable to read native runtime status:", error);
      setRuntime(null);
    } finally {
      setChecking(false);
    }
  }, [isDesktop]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { runtime, checking, isDesktop, refresh };
}

function usePerformanceMetrics(isDesktop: boolean) {
  const [snapshot, setSnapshot] = useState<PerformanceSnapshot | null>(null);
  const [checking, setChecking] = useState(true);

  const refresh = useCallback(async () => {
    if (!isDesktop) {
      setChecking(false);
      return;
    }
    setChecking(true);
    try {
      setSnapshot(await invoke<PerformanceSnapshot>("get_performance_snapshot"));
    } catch (error) {
      console.error("Unable to read performance metrics:", error);
      setSnapshot(null);
    } finally {
      setChecking(false);
    }
  }, [isDesktop]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { snapshot, checking, refresh };
}

function Topbar({
  page,
  isDesktop,
  playerName,
  onNavigate
}: {
  page: Page;
  isDesktop: boolean;
  playerName: string;
  onNavigate: (page: Page) => void;
}) {
  const primary = [
    { id: "overview" as Page, label: "Home" },
    { id: "instances" as Page, label: "Versions" },
    { id: "accounts" as Page, label: "Accounts" }
  ];

  return (
    <header className="topbar nova-topbar">
      <button className="nova-brand" onClick={() => onNavigate("overview")} aria-label="NOVA home">
        <span className="nova-brand-mark">N</span>
        <span className="nova-brand-word">NOVA</span>
      </button>

      <nav className="nova-primary-nav" aria-label="Main navigation">
        {primary.map((item) => (
          <button
            key={item.id}
            className={page === item.id ? "nova-nav-link active" : "nova-nav-link"}
            onClick={() => onNavigate(item.id)}
            aria-current={page === item.id ? "page" : undefined}
          >
            {item.label}
          </button>
        ))}
      </nav>

      <div className="topbar-actions nova-topbar-actions">
        <span className={isDesktop ? "connection-chip connected" : "connection-chip"}>
          <span className="connection-dot" />
          {isDesktop ? "Ready" : "Preview"}
        </span>
        <button className="icon-button nova-tool-button" onClick={() => onNavigate("performance")} aria-label="Performance" title="Performance">
          <Gauge size={17} />
        </button>
        <button className="icon-button nova-tool-button" onClick={() => onNavigate("diagnostics")} aria-label="Diagnostics" title="Diagnostics">
          <ScanSearch size={17} />
        </button>
        <button className="icon-button nova-tool-button" onClick={() => onNavigate("settings")} aria-label="Settings" title="Settings">
          <Settings2 size={17} />
        </button>
        <button className="nova-account-shortcut" onClick={() => onNavigate("accounts")} title="Manage local player profile">
          <span className="topbar-avatar">{playerName.slice(0, 1).toUpperCase() || "N"}</span>
          <span>{playerName}</span>
        </button>
      </div>
    </header>
  );
}

function HomePage({
  isDesktop,
  playerName,
  onNavigate
}: {
  isDesktop: boolean;
  playerName: string;
  onNavigate: (page: Page) => void;
}) {
  const [catalog, setCatalog] = useState<VersionCatalog | null>(null);
  const [instances, setInstances] = useState<InstalledInstance[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [loading, setLoading] = useState(true);
  const [launching, setLaunching] = useState(false);
  const [session, setSession] = useState<LaunchSession | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState("Ready to play");

  useEffect(() => {
    if (!isDesktop) {
      setLoading(false);
      return;
    }
    let disposed = false;
    const load = async () => {
      setLoading(true);
      setError(null);
      const [catalogResult, instancesResult, sessionsResult] = await Promise.allSettled([
        invoke<VersionCatalog>("get_minecraft_versions"),
        invoke<InstalledInstance[]>("list_installed_instances"),
        invoke<LaunchSession[]>("list_launch_sessions")
      ]);
      if (disposed) return;
      if (catalogResult.status === "fulfilled") setCatalog(catalogResult.value);
      else setError(String(catalogResult.reason));
      if (instancesResult.status === "fulfilled") {
        setInstances(instancesResult.value);
        setSelectedId((current) => current || instancesResult.value[0]?.id || "");
      } else {
        setError((current) => current || String(instancesResult.reason));
      }
      if (sessionsResult.status === "fulfilled") {
        const active = sessionsResult.value[0] ?? null;
        setSession(active);
        setStatus(active ? `Minecraft is running · PID ${active.pid}` : "Ready to play");
      }
      setLoading(false);
    };
    void load();
    return () => { disposed = true; };
  }, [isDesktop]);

  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<ProcessState>("nova-process-state", (event) => {
      const payload = event.payload;
      setStatus(payload.message || payload.state);
      setLaunching(false);
      if (payload.state === "running") {
        setSession({
          instanceId: payload.instanceId,
          pid: payload.pid ?? 0,
          startedAtUnix: Math.floor(Date.now() / 1000),
          state: "running"
        });
      } else {
        setSession(null);
        if (payload.state === "failed") setError(payload.message || "Minecraft failed to start.");
      }
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((cause: unknown) => console.error("Unable to subscribe to Minecraft process state:", cause));
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isDesktop]);

  const selected = instances.find((instance) => instance.id === selectedId) ?? instances[0] ?? null;
  const featuredVersion = selected?.id ?? catalog?.latest.release ?? "Minecraft";
  const featuredType = selected?.versionType ?? "Java Edition";

  const launch = async () => {
    if (!isDesktop) {
      setError("Launch Minecraft from the installed NOVA desktop application.");
      return;
    }
    if (!selected) {
      onNavigate("instances");
      return;
    }
    if (session) {
      try {
        await invoke("stop_minecraft", { instanceId: session.instanceId });
        setStatus("Stopping Minecraft…");
      } catch (cause) {
        setError(String(cause));
      }
      return;
    }
    setLaunching(true);
    setError(null);
    setStatus("Preparing Minecraft…");
    try {
      const nextSession = await invoke<LaunchSession>("launch_minecraft", {
        instanceId: selected.id,
        playerName,
        memoryMb: 4096
      });
      setSession(nextSession);
      setStatus(`Minecraft is running · PID ${nextSession.pid}`);
    } catch (cause) {
      setError(String(cause));
      setStatus("Launch failed");
      setLaunching(false);
    }
  };

  return (
    <div className="nova-home">
      <section className="nova-hero">
        <div className="nova-hero-scene" aria-hidden="true">
          <div className="nova-sky-glow" />
          <div className="nova-cloud cloud-one" />
          <div className="nova-cloud cloud-two" />
          <div className="nova-sun" />
          <div className="nova-mountain mountain-back" />
          <div className="nova-mountain mountain-front" />
          <div className="nova-floating-island">
            <span className="island-grass" />
            <span className="island-dirt" />
            <span className="island-stone" />
            <span className="island-tree" />
          </div>
          <div className="nova-scene-vignette" />
        </div>

        <div className="nova-hero-content">
          <div className="nova-kicker"><span /> YOUR MINECRAFT, ONE PLACE</div>
          <div className="nova-release-label">{selected ? "SELECTED VERSION" : "LATEST OFFICIAL RELEASE"}</div>
          <h1>{featuredVersion}</h1>
          <p className="nova-hero-subtitle">{selected ? `${featuredType} · Ready to launch` : "Your next adventure starts here."}</p>

          <div className="nova-hero-controls">
            {instances.length > 0 ? (
              <select
                className="nova-version-select"
                value={selected?.id ?? ""}
                onChange={(event) => setSelectedId(event.target.value)}
                disabled={loading || Boolean(session) || launching}
                aria-label="Select installed Minecraft version"
              >
                {instances.map((instance) => (
                  <option key={instance.id} value={instance.id}>{instance.id} · {instance.versionType}</option>
                ))}
              </select>
            ) : (
              <span className="nova-no-install">{loading ? "Checking installed versions…" : "No Minecraft version installed"}</span>
            )}
            <button
              className="nova-launch-button"
              onClick={() => void launch()}
              disabled={loading || launching || Boolean(session) || (!selected && !isDesktop)}
            >
              <Play size={17} fill="currentColor" />
              {launching ? "Starting…" : session ? "Running" : selected ? "Launch" : "Choose version"}
            </button>
            {session && (
              <button className="nova-stop-button" onClick={() => void launch()} title="Stop Minecraft">
                <Square size={15} fill="currentColor" />
              </button>
            )}
          </div>

          <div className="nova-hero-meta">
            <span><UserRound size={13} /> {playerName}</span>
            <span><Activity size={13} /> {status}</span>
          </div>
          {error && <div className="nova-home-error" role="alert">{error}</div>}
          {!isDesktop && <p className="nova-preview-note">Desktop mode is required for installation and launching.</p>}
        </div>

        <div className="nova-hero-stamp" aria-hidden="true">
          <span>JAVA</span>
          <strong>EDITION</strong>
          <i />
          <small>PLAY YOUR WAY</small>
        </div>
      </section>

      <section className="nova-library-section">
        <div className="nova-section-heading">
          <div>
            <span className="section-eyebrow">YOUR GAME LIBRARY</span>
            <h2>Installed versions</h2>
          </div>
          <button className="nova-link-button" onClick={() => onNavigate("instances")}>
            Manage versions <ArrowRight size={15} />
          </button>
        </div>

        {instances.length > 0 ? (
          <div className="nova-version-grid">
            {instances.slice(0, 4).map((instance, index) => (
              <button
                type="button"
                key={instance.id}
                className={selected?.id === instance.id ? "nova-version-card selected" : "nova-version-card"}
                onClick={() => setSelectedId(instance.id)}
              >
                <span className={`nova-version-art art-${index % 4}`}>
                  <i className="version-sun" />
                  <i className="version-hill hill-a" />
                  <i className="version-hill hill-b" />
                  <i className="version-cube" />
                  <span className="nova-version-badge">{instance.versionType}</span>
                </span>
                <span className="nova-version-card-copy">
                  <strong>{instance.id}</strong>
                  <small>{formatBytes(instance.totalBytes)} · {instance.files} verified files</small>
                </span>
                <span className="nova-card-arrow"><ArrowRight size={15} /></span>
              </button>
            ))}
            <button className="nova-add-version" onClick={() => onNavigate("instances")}>
              <span><Layers3 size={21} /></span>
              <strong>Add a version</strong>
              <small>Vanilla releases</small>
            </button>
          </div>
        ) : (
          <div className="nova-empty-library">
            <div className="nova-empty-icon"><Layers3 size={22} /></div>
            <div>
              <strong>{loading ? "Loading your library…" : "Your library is empty"}</strong>
              <p>Install an official Minecraft version to make it appear here. You can keep multiple versions separate.</p>
            </div>
            <button className="nova-link-button" onClick={() => onNavigate("instances")}>
              Browse versions <ArrowRight size={15} />
            </button>
          </div>
        )}
      </section>

      <section className="nova-quick-strip">
        <button onClick={() => onNavigate("accounts")}><UserRound size={17} /><span><strong>Player profile</strong><small>Offline name · {playerName}</small></span><ArrowRight size={14} /></button>
        <button onClick={() => onNavigate("performance")}><Gauge size={17} /><span><strong>Performance</strong><small>Native runtime metrics</small></span><ArrowRight size={14} /></button>
        <button onClick={() => onNavigate("diagnostics")}><ScanSearch size={17} /><span><strong>Diagnostics</strong><small>Check your installation</small></span><ArrowRight size={14} /></button>
      </section>
    </div>
  );
}

function InstancesPage(function InstancesPage({ isDesktop, playerName, setPlayerName }: {
  isDesktop: boolean;
  playerName: string;
  setPlayerName: (value: string) => void;
}) {
  const [catalog, setCatalog] = useState<VersionCatalog | null>(null);
  const [instances, setInstances] = useState<InstalledInstance[]>([]);
  const [selectedVersion, setSelectedVersion] = useState("");
  const [includeSnapshots, setIncludeSnapshots] = useState(false);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [memoryMb, setMemoryMb] = useState(4096);
  const [launchSession, setLaunchSession] = useState<LaunchSession | null>(null);
  const [launching, setLaunching] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const [launchStatus, setLaunchStatus] = useState("No game process");
  const [logs, setLogs] = useState<string[]>([]);

  const refresh = useCallback(async () => {
    if (!isDesktop) {
      setLoading(false);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const [nextCatalog, nextInstances, nextSessions] = await Promise.all([
        invoke<VersionCatalog>("get_minecraft_versions"),
        invoke<InstalledInstance[]>("list_installed_instances"),
        invoke<LaunchSession[]>("list_launch_sessions")
      ]);
      setCatalog(nextCatalog);
      setInstances(nextInstances);
      setLaunchSession(nextSessions[0] ?? null);
      setSelectedVersion((current) => current || nextCatalog.latest.release);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setLoading(false);
    }
  }, [isDesktop]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!isDesktop) return;
    let unlisten: (() => void) | undefined;
    let disposed = false;
    void listen<InstallProgress>("nova-install-progress", (event) => {
      setProgress(event.payload);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((cause: unknown) => {
      console.error("Unable to subscribe to installation progress:", cause);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isDesktop]);

  const install = useCallback(async () => {
    if (!selectedVersion || installing) return;
    setInstalling(true);
    setProgress(null);
    setError(null);
    try {
      await invoke<InstalledInstance>("install_vanilla_version", {
        versionId: selectedVersion
      });
      const nextInstances = await invoke<InstalledInstance[]>("list_installed_instances");
      setInstances(nextInstances);
      setProgress((current) => current ? { ...current, phase: "completed" } : current);
    } catch (cause) {
      setError(String(cause));
    } finally {
      setInstalling(false);
    }
  }, [installing, selectedVersion]);

  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let stopLogs: (() => void) | undefined;
    let stopState: (() => void) | undefined;
    void listen<ProcessLog>("nova-process-log", (event) => {
      const payload = event.payload;
      setLogs((current) => [...current, `[${payload.stream}] ${payload.line}`].slice(-180));
    }).then((stop) => {
      if (disposed) stop();
      else stopLogs = stop;
    }).catch((cause: unknown) => console.error("Unable to subscribe to Minecraft logs:", cause));
    void listen<ProcessState>("nova-process-state", (event) => {
      const payload = event.payload;
      setLaunchStatus(payload.message || payload.state);
      if (payload.state === "running") {
        setLaunchSession({ instanceId: payload.instanceId, pid: payload.pid ?? 0, startedAtUnix: Math.floor(Date.now() / 1000), state: "running" });
        setLaunching(false);
      } else {
        setLaunchSession(null);
        setLaunching(false);
        if (payload.state === "failed") setLaunchError(payload.message || "Minecraft process failed");
      }
    }).then((stop) => {
      if (disposed) stop();
      else stopState = stop;
    }).catch((cause: unknown) => console.error("Unable to subscribe to Minecraft process state:", cause));
    return () => {
      disposed = true;
      stopLogs?.();
      stopState?.();
    };
  }, [isDesktop]);

  const launch = useCallback(async (instanceId: string) => {
    if (launching || launchSession) return;
    setLaunching(true);
    setLaunchError(null);
    setLaunchStatus("Preparing Minecraft process…");
    setLogs([]);
    try {
      const session = await invoke<LaunchSession>("launch_minecraft", { instanceId, playerName, memoryMb });
      setLaunchSession(session);
      setLaunchStatus(`Minecraft running · PID ${session.pid}`);
    } catch (cause) {
      setLaunchError(String(cause));
      setLaunchStatus("Launch failed");
      setLaunching(false);
    }
  }, [launching, launchSession, playerName, memoryMb]);

  const stop = useCallback(async (instanceId: string) => {
    try {
      await invoke("stop_minecraft", { instanceId });
      setLaunchStatus("Stopping Minecraft…");
    } catch (cause) {
      setLaunchError(String(cause));
    }
  }, []);

  const releases = catalog?.versions.filter((version) => version.versionType === "release").slice(0, 40) ?? [];
  const snapshots = catalog?.versions.filter((version) => version.versionType === "snapshot").slice(0, 20) ?? [];
  const selected = catalog?.versions.find((version) => version.id === selectedVersion);
  const progressPercent = progress
    ? progress.phase === "completed"
      ? 100
      : progress.totalFiles > 0
        ? Math.min(99, Math.round((progress.completedFiles / progress.totalFiles) * 100))
        : 0
    : 0;

  if (!isDesktop) {
    return (
      <EmptyPage
        icon={Layers3}
        eyebrow="NATIVE INSTANCE ENGINE"
        title="Open NOVA Desktop to install Minecraft."
        description="The browser preview cannot write game files. The desktop build connects to Mojang, verifies every artifact and stores shared assets outside each isolated instance."
        actionLabel="Back to overview"
      />
    );
  }

  return (
    <div className="instance-manager">
      <section className="panel instance-installer">
        <div className="instance-installer-toolbar">
          <div>
            <span className="section-eyebrow">OFFICIAL MOJANG CATALOGUE</span>
            <h2>Install a Vanilla version</h2>
            <p>Official release metadata, SHA-1 checked client files, shared libraries and a deduplicated asset store. Each version receives its own instance directory.</p>
          </div>
          <button className="icon-button" onClick={() => void refresh()} disabled={loading || installing} aria-label="Refresh Minecraft versions">
            <RefreshCw size={16} className={loading ? "spin" : ""} />
          </button>
        </div>
        <div className="instance-meta-line">
          <span>Latest release · {catalog?.latest.release ?? "—"}</span>
          <span>Latest snapshot · {catalog?.latest.snapshot ?? "—"}</span>
          <span>{loading ? "SYNCING CATALOGUE" : catalog ? "MOJANG METADATA READY" : "CATALOGUE UNAVAILABLE"}</span>
        </div>
        <div className="instance-catalog-controls">
          <select
            className="instance-select"
            value={selectedVersion}
            onChange={(event) => setSelectedVersion(event.target.value)}
            disabled={loading || installing || !catalog}
            aria-label="Minecraft version"
          >
            <optgroup label="Official releases">
              {releases.map((version) => <option key={version.id} value={version.id}>{version.id}</option>)}
            </optgroup>
            {includeSnapshots && (
              <optgroup label="Snapshots">
                {snapshots.map((version) => <option key={version.id} value={version.id}>{version.id}</option>)}
              </optgroup>
            )}
          </select>
          <button className="instance-install-button" onClick={() => void install()} disabled={loading || installing || !selectedVersion}>
            {installing ? <RefreshCw size={15} className="spin" /> : <Download size={15} />}
            {installing ? "Installing…" : "Install Vanilla"}
          </button>
        </div>
        <div className="instance-meta-line">
          <label><input type="checkbox" checked={includeSnapshots} onChange={(event) => setIncludeSnapshots(event.target.checked)} disabled={installing} /> Include snapshots in selector</label>
          <span>{selected ? selected.versionType.toUpperCase() + " · " + new Date(selected.releaseTime).toLocaleDateString() : "Select a version"}</span>
        </div>
        {installing && progress && (
          <div className="instance-progress">
            <div className="instance-progress-head">
              <span>{progress.phase.replace("-", " ").toUpperCase()}</span>
              <strong>{progressPercent}%</strong>
            </div>
            <div className="instance-progress-track">
              <div className="instance-progress-fill" style={{ width: progressPercent + "%" }} />
            </div>
            <div className="instance-progress-detail">
              <span>{progress.completedFiles}/{progress.totalFiles} files · {formatBytes(progress.transferredBytes)} current file</span>
              <span>{progress.currentFile}</span>
            </div>
          </div>
        )}
        {error && <div className="instance-error" role="alert">{error}</div>}
        <p className="instance-catalog-note">NOVA stores reusable assets and libraries in a shared cache. An existing artifact is reused only after its size and SHA-1 have been verified.</p>
      </section>

      <section className="panel instance-installed">
        <div className="panel-heading">
          <div><span className="section-eyebrow">LOCAL GAME DATA</span><h2>Installed instances</h2></div>
          <span className="value-tag success">{instances.length} INSTALLED</span>
        </div>
        {instances.length === 0 ? (
          <div className="instance-empty-state">
            <Layers3 size={22} />
            <span>No verified instances yet. Choose a release above to create your first one.</span>
          </div>
        ) : (
          <div className="instance-installed-grid">
            {instances.map((instance) => (
              <article className="instance-card" key={instance.id}>
                <div className="instance-card-top">
                  <strong>{instance.id}</strong>
                  <span className="instance-type">{instance.versionType}</span>
                </div>
                <div className="instance-card-meta">
                  <span>{formatBytes(instance.totalBytes)}</span>
                  <span>{instance.files} verified files</span>
                  <span>Assets {instance.assetIndex}</span>
                </div>
                <div className="instance-card-path">{instance.instancePath}</div>
                <div className="instance-card-actions">
                  {launchSession?.instanceId === instance.id ? (
                    <button className="instance-process-button stop" onClick={() => void stop(instance.id)}>
                      <Square size={13} fill="currentColor" /> Stop game
                    </button>
                  ) : (
                    <button className="instance-process-button" onClick={() => void launch(instance.id)} disabled={launching || Boolean(launchSession) || installing}>
                      {launching ? <RefreshCw size={13} className="spin" /> : <Play size={13} fill="currentColor" />}
                      {launching ? "Starting…" : "Launch"}
                    </button>
                  )}
                  {launchSession?.instanceId === instance.id && <span className="instance-running-pid">PID {launchSession.pid}</span>}
                </div>
              </article>
            ))}
          </div>
        )}
      </section>

      <section className="panel launch-console">
        <div className="panel-heading">
          <div><span className="section-eyebrow">NATIVE PROCESS CONTROL</span><h2>Launch configuration</h2></div>
          <span className={launchSession ? "value-tag success" : "value-tag"}>{launchSession ? "RUNNING" : launchStatus.toUpperCase()}</span>
        </div>
        <div className="launch-config-grid">
          <label className="launch-config-field">
            <span>Offline player name</span>
            <input value={playerName} onChange={(event) => setPlayerName(event.target.value)} maxLength={16} spellCheck={false} disabled={launching || Boolean(launchSession)} />
          </label>
          <label className="launch-config-field">
            <span>Memory allocation</span>
            <select value={memoryMb} onChange={(event) => setMemoryMb(Number(event.target.value))} disabled={launching || Boolean(launchSession)}>
              {[2048, 3072, 4096, 6144, 8192, 12288, 16384].map((value) => <option key={value} value={value}>{formatBytes(value * 1024 * 1024)} RAM</option>)}
            </select>
          </label>
        </div>
        <p className="launch-offline-note">Offline profile · no Microsoft sign-in yet. Offline-mode servers only; authenticated multiplayer requires Microsoft account support.</p>
        {launchError && <div className="instance-error" role="alert">{launchError}</div>}
        <div className="launch-log-heading"><Terminal size={14} /><span>LIVE GAME OUTPUT</span><small>{logs.length} lines captured</small></div>
        <pre className="launch-log-output" aria-live="polite">{logs.length ? logs.join("\n") : "Minecraft stdout/stderr will appear here after launch."}</pre>
      </section>
    </div>
  );
}

function EmptyPage({
  icon: Icon,
  title,
  description,
  eyebrow,
  actionLabel,
  onAction
}: {
  icon: LucideIcon;
  title: string;
  description: string;
  eyebrow: string;
  actionLabel: string;
  onAction?: () => void;
}) {
  return (
    <section className="page-empty panel">
      <div className="empty-orbit">
        <div className="empty-orbit-core"><Icon size={28} strokeWidth={1.5} /></div>
      </div>
      <span className="section-eyebrow">{eyebrow}</span>
      <h2>{title}</h2>
      <p>{description}</p>
      {onAction && <button className="button button-secondary" onClick={onAction}>{actionLabel} <ArrowRight size={15} /></button>}
      <div className="feature-state"><span className="state-dot" /> ENGINE IN DEVELOPMENT</div>
    </section>
  );
}


function formatBytes(bytes: number): string {
  if (bytes < 1024) return String(bytes) + " B";
  const units = ["KB", "MB", "GB", "TB"];
  let value = bytes / 1024;
  let unit = 0;
  while (value >= 1024 && unit < units.length - 1) {
    value /= 1024;
    unit += 1;
  }
  return value.toFixed(1) + " " + units[unit];
}

function PerformanceStat({ icon: Icon, label, value, note }: {
  icon: LucideIcon;
  label: string;
  value: string;
  note: string;
}) {
  return (
    <article className="performance-stat">
      <div className="performance-stat-icon"><Icon size={17} /></div>
      <div className="performance-stat-copy">
        <span>{label}</span>
        <strong>{value}</strong>
        <small>{note}</small>
      </div>
    </article>
  );
}

function PerformancePage({ snapshot, checking, isDesktop, onRefresh }: {
  snapshot: PerformanceSnapshot | null;
  checking: boolean;
  isDesktop: boolean;
  onRefresh: () => void;
}) {
  if (!isDesktop) {
    return (
      <EmptyPage
        icon={Gauge}
        eyebrow="NATIVE TELEMETRY"
        title="Metrics need the desktop runtime."
        description="Run NOVA as a Tauri application to read live native performance counters. Browser preview does not fabricate system metrics."
        actionLabel="Back to overview"
      />
    );
  }

  const totalCacheRequests = (snapshot?.cacheHits ?? 0) + (snapshot?.cacheMisses ?? 0);
  const hitRate = snapshot ? (snapshot.cacheHitRateBasisPoints / 100).toFixed(1) + "%" : "—";

  return (
    <div className="performance-content">
      <div className="performance-toolbar">
        <div>
          <span className="section-eyebrow">NATIVE INSTRUMENTATION</span>
          <p>Session counters from the Rust runtime. Values are real and remain zero until the related service is implemented.</p>
        </div>
        <button className="icon-button" onClick={onRefresh} disabled={checking} aria-label="Refresh performance metrics">
          <RefreshCw size={16} className={checking ? "spin" : ""} />
        </button>
      </div>
      <section className="performance-grid" aria-label="Performance counters">
        <PerformanceStat icon={Activity} label="ACTIVE OPERATIONS" value={snapshot ? String(snapshot.activeOperations) : "—"} note="Native tasks currently tracked" />
        <PerformanceStat icon={Gauge} label="PEAK CONCURRENCY" value={snapshot ? String(snapshot.peakOperations) : "—"} note="Highest simultaneous tracked tasks" />
        <PerformanceStat icon={CheckCircle2} label="COMPLETED OPERATIONS" value={snapshot ? String(snapshot.completedOperations) : "—"} note="Tracked task scopes completed" />
        <PerformanceStat icon={Layers3} label="CACHE HIT RATE" value={hitRate} note={totalCacheRequests ? String(totalCacheRequests) + " measured lookups" : "No cache lookups recorded"} />
      </section>
      <section className="panel performance-panel">
        <div className="panel-heading">
          <div><span className="section-eyebrow">SESSION TELEMETRY</span><h2>Runtime breakdown</h2></div>
          <span className="telemetry-state"><i /> {checking ? "REFRESHING" : snapshot ? "CONNECTED" : "UNAVAILABLE"}</span>
        </div>
        <div className="performance-rows">
          <div className="settings-row"><div className="settings-row-copy"><strong>Session uptime</strong><span>Since native metrics were initialized</span></div><div className="settings-row-value">{snapshot ? (snapshot.uptimeMs / 1000).toFixed(1) + " s" : "—"}</div></div>
          <div className="settings-row"><div className="settings-row-copy"><strong>Cumulative operation time</strong><span>Sum of tracked operation durations, not launch time</span></div><div className="settings-row-value">{snapshot ? String(snapshot.totalOperationTimeMs) + " ms" : "—"}</div></div>
          <div className="settings-row"><div className="settings-row-copy"><strong>Downloaded bytes</strong><span>Bytes transferred by instrumented download services</span></div><div className="settings-row-value">{snapshot ? formatBytes(snapshot.downloadedBytes) : "—"}</div></div>
          <div className="settings-row"><div className="settings-row-copy"><strong>Reused bytes</strong><span>Bytes served from verified local content</span></div><div className="settings-row-value">{snapshot ? formatBytes(snapshot.reusedBytes) : "—"}</div></div>
          <div className="settings-row"><div className="settings-row-copy"><strong>Integrity verification failures</strong><span>Failed artifact checks reported by the engine</span></div><div className={snapshot?.verificationFailures ? "value-tag" : "value-tag success"}>{snapshot ? snapshot.verificationFailures : "—"}</div></div>
        </div>
        <p className="performance-disclaimer">This is native instrumentation, not an FPS benchmark. Download, cache and integrity counters become meaningful when those real engine services are connected.</p>
      </section>
    </div>
  );
}


function DiagnosticsPage({
  report,
  loading,
  error,
  isDesktop,
  onRun,
  onOpenFolder
}: {
  report: DiagnosticReport | null;
  loading: boolean;
  error: string | null;
  isDesktop: boolean;
  onRun: (deep: boolean) => void;
  onOpenFolder: () => void;
}) {
  if (!isDesktop) {
    return (
      <EmptyPage
        icon={ScanSearch}
        eyebrow="LOCAL DIAGNOSTICS"
        title="Open NOVA Desktop to inspect the system."
        description="The diagnostic engine runs locally inside the desktop application. Browser preview cannot access Java, game files, hardware details or crash logs."
        actionLabel="Back to overview"
      />
    );
  }

  const status = report?.summary.overallStatus ?? "unknown";
  const statusLabel = status === "critical" ? "ISSUES FOUND" : status === "warning" ? "ATTENTION" : status === "healthy" ? "HEALTHY" : "WAITING FOR SCAN";
  const gpus = report?.system.gpus ?? [];
  const formatTime = (timestamp: number) => new Date(timestamp * 1000).toLocaleString();

  return (
    <div className="diagnostic-page">
      <section className="panel diagnostic-hero">
        <div className="diagnostic-hero-copy">
          <span className="section-eyebrow">NOVA DIAGNOSTIC ENGINE · LOCAL ONLY</span>
          <h2>{report ? "System health report" : "Know exactly what is wrong."}</h2>
          <p>{report ? `Report ${report.reportId} · ${report.mode.toUpperCase()} scan · ${formatTime(report.generatedAtUnix)}` : "Run a quick check or a full integrity scan of every installed instance."}</p>
        </div>
        <span className={`diagnostic-status ${status}`}>{loading ? "SCANNING…" : statusLabel}</span>
        <div className="diagnostic-actions">
          <button className="instance-process-button" onClick={() => onRun(false)} disabled={loading}>
            <RefreshCw size={14} className={loading ? "spin" : ""} /> Quick scan
          </button>
          <button className="instance-install-button" onClick={() => onRun(true)} disabled={loading}>
            <ScanSearch size={14} /> Deep integrity scan
          </button>
          <button className="icon-button" onClick={onOpenFolder} disabled={loading} aria-label="Open diagnostic reports folder" title="Open saved reports folder">
            <FolderOpen size={16} />
          </button>
        </div>
        {error && <div className="instance-error" role="alert">{error}</div>}
        <p className="diagnostic-mode-note">Quick scan: system inventory, Java detection, client SHA-1 and file presence. Deep scan: additionally hashes all indexed assets and applicable libraries. A deep scan can take longer on large installations.</p>
      </section>

      {report && (
        <>
          <section className="diagnostic-summary-grid">
            <article className="diagnostic-stat"><span>CHECKS</span><strong>{report.summary.checksTotal}</strong><small>Total diagnostic checks</small></article>
            <article className="diagnostic-stat passed"><span>PASSED</span><strong>{report.summary.passed}</strong><small>Checks with expected results</small></article>
            <article className="diagnostic-stat warning"><span>WARNINGS</span><strong>{report.summary.warnings}</strong><small>Potential issues or unavailable data</small></article>
            <article className="diagnostic-stat failed"><span>FAILURES</span><strong>{report.summary.failures}</strong><small>Problems requiring attention</small></article>
          </section>

          <section className="panel diagnostic-section">
            <div className="panel-heading">
              <div><span className="section-eyebrow">MACHINE PROFILE</span><h2>Hardware & operating system</h2></div>
              <span className="value-tag">{report.system.architecture}</span>
            </div>
            <div className="diagnostic-hardware-grid">
              <div><span>OPERATING SYSTEM</span><strong>{report.system.operatingSystem}</strong><small>{report.system.osVersion ?? "Version unavailable"}{report.system.osBuild ? " · Build " + report.system.osBuild : ""}</small></div>
              <div><span>PROCESSOR</span><strong>{report.system.cpuModel ?? "Not exposed"}</strong><small>{report.system.logicalProcessors} logical processors</small></div>
              <div><span>PHYSICAL MEMORY</span><strong>{report.system.totalMemoryBytes ? formatBytes(report.system.totalMemoryBytes) : "Not exposed"}</strong><small>Detected by the operating-system inventory</small></div>
              <div><span>GPU DEVICES</span><strong>{gpus.length || "Not detected"}</strong><small>{report.system.hardwareQueryStatus}</small></div>
            </div>
            {gpus.length > 0 && (
              <div className="diagnostic-gpu-list">
                {gpus.map((gpu, index) => (
                  <div className="diagnostic-gpu" key={gpu.name + index}>
                    <Cpu size={15} />
                    <div><strong>{gpu.name}</strong><small>Driver {gpu.driverVersion ?? "unknown"}{gpu.adapterMemoryBytes ? " · " + formatBytes(gpu.adapterMemoryBytes) : ""}</small></div>
                  </div>
                ))}
              </div>
            )}
          </section>

          <section className="panel diagnostic-section">
            <div className="panel-heading">
              <div><span className="section-eyebrow">NETWORK PATH</span><h2>Mojang & Minecraft services</h2></div>
              <span className="value-tag">{report.network.filter((endpoint) => endpoint.reachable).length}/{report.network.length} REACHABLE</span>
            </div>
            <div className="diagnostic-network-list">
              {report.network.map((endpoint) => (
                <div className="diagnostic-network-row" key={endpoint.host}>
                  <span className={endpoint.reachable ? "network-state good" : "network-state bad"} />
                  <div><strong>{endpoint.name}</strong><small>{endpoint.host}</small></div>
                  <span className={endpoint.reachable ? "diagnostic-pill passed" : "diagnostic-pill warning"}>{endpoint.statusCode ? "HTTP " + endpoint.statusCode : "NO RESPONSE"}</span>
                  <small>{endpoint.latencyMs === null ? "—" : endpoint.latencyMs + " ms"}</small>
                </div>
              ))}
            </div>
            <p className="diagnostic-network-note">These checks send small requests to public Minecraft endpoints (which will see your public IP, as with any network request). They do not transmit the diagnostic report, account details or authentication tokens.</p>
          </section>

          <section className="panel diagnostic-section">
            <div className="panel-heading">
              <div><span className="section-eyebrow">INSTANCE INTEGRITY</span><h2>Minecraft installations</h2></div>
              <span className="value-tag">{report.instances.length} SCANNED</span>
            </div>
            {report.instances.length === 0 ? (
              <div className="instance-empty-state"><Layers3 size={20} /><span>No NOVA-managed Minecraft instances were found.</span></div>
            ) : report.instances.map((instance) => (
              <article className="diagnostic-instance" key={instance.id}>
                <div className="diagnostic-instance-heading">
                  <strong>{instance.id}</strong>
                  <span className={instance.clientStatus === "verified" ? "diagnostic-pill passed" : "diagnostic-pill failed"}>{instance.clientStatus.replace("_", " ").toUpperCase()}</span>
                </div>
                <div className="diagnostic-instance-metrics">
                  <span>Java required: <b>{instance.javaRequiredMajor ?? "Unknown"}</b></span>
                  <span>Libraries: <b>{instance.libraryCount}</b> · Missing <b>{instance.missingLibraries}</b> · Invalid <b>{instance.invalidLibraries}</b></span>
                  <span>Assets: <b>{instance.assetCount}</b> · Missing <b>{instance.missingAssets}</b> · Invalid <b>{instance.invalidAssets}</b></span>
                  <span>Native files: <b>{instance.nativeFileCount}</b></span>
                </div>
                <div className="diagnostic-java-list">
                  {instance.javaCandidates.map((java, index) => (
                    <div key={java.executable + index}>
                      <span className={java.compatible ? "java-dot good" : "java-dot"} />
                      <span>{java.source}: {java.version ?? "unavailable"}</span>
                      <small>{java.executable}</small>
                    </div>
                  ))}
                  {instance.javaCandidates.length === 0 && <small>No Java candidates were detected.</small>}
                </div>
                <div className="diagnostic-instance-hashes">
                  <span>Client SHA-1 expected: {instance.clientExpectedSha1 ?? "unavailable"}</span>
                  <span>Client SHA-1 actual: {instance.clientActualSha1 ?? "unavailable"}</span>
                </div>
                {instance.crashArtifactNames.length > 0 && <p className="diagnostic-crash-files">Crash evidence: {instance.crashArtifactNames.join(", ")}</p>}
                {instance.rootCauseHypotheses.length > 0 && (
                  <div className="diagnostic-hypotheses">
                    <div className="diagnostic-hypotheses-heading"><AlertCircle size={14} /><strong>LIKELY CAUSES · HYPOTHESES</strong></div>
                    {instance.rootCauseHypotheses.map((diagnosis) => (
                      <article className="diagnostic-hypothesis" key={diagnosis.code}>
                        <div className="diagnostic-hypothesis-top">
                          <strong>{diagnosis.title}</strong>
                          <span className={diagnosis.severity === "critical" ? "diagnostic-pill failed" : "diagnostic-pill warning"}>{diagnosis.confidence.toUpperCase()} CONFIDENCE</span>
                        </div>
                        <p>{diagnosis.likelyCause}</p>
                        {diagnosis.evidence.length > 0 && <details><summary>Matched log lines ({diagnosis.evidence.length})</summary><pre>{diagnosis.evidence.join("\\n")}</pre></details>}
                        <div className="diagnostic-recommendation"><b>Suggested next step:</b> {diagnosis.recommendedAction}</div>
                      </article>
                    ))}
                  </div>
                )}
                {instance.crashReportExcerpt.length > 0 && (
                  <details className="diagnostic-log-details">
                    <summary>Latest JVM / Minecraft crash excerpt ({instance.crashReportExcerpt.length} lines)</summary>
                    <pre>{instance.crashReportExcerpt.join("\n")}</pre>
                  </details>
                )}
                {instance.latestLogExcerpt.length > 0 && (
                  <details className="diagnostic-log-details">
                    <summary>latest.log excerpt ({instance.latestLogExcerpt.length} lines)</summary>
                    <pre>{instance.latestLogExcerpt.join("\n")}</pre>
                  </details>
                )}
              </article>
            ))}
          </section>

          <section className="panel diagnostic-section">
            <div className="panel-heading">
              <div><span className="section-eyebrow">DETAILED RESULTS</span><h2>Checks & recommendations</h2></div>
              <span className="value-tag">{report.checks.length} CHECKS</span>
            </div>
            <div className="diagnostic-check-list">
              {report.checks.map((check) => (
                <article className={`diagnostic-check ${check.status}`} key={check.id}>
                  <div className="diagnostic-check-icon">
                    {check.status === "passed" ? <CheckCircle2 size={16} /> : check.status === "failed" ? <ShieldCheck size={16} /> : <AlertCircle size={16} />}
                  </div>
                  <div className="diagnostic-check-copy">
                    <div className="diagnostic-check-title"><strong>{check.title}</strong><span className={`diagnostic-pill ${check.status}`}>{check.status.toUpperCase()}</span><small>{check.category} · {check.severity}</small></div>
                    <p>{check.details}</p>
                    {check.evidence.length > 0 && <details><summary>Evidence ({check.evidence.length})</summary><pre>{check.evidence.join("\n")}</pre></details>}
                    {check.recommendedAction && <div className="diagnostic-recommendation"><b>Recommended action:</b> {check.recommendedAction}</div>}
                  </div>
                </article>
              ))}
            </div>
          </section>

          {report.rootCauseHypotheses.length > 0 && (
            <section className="panel diagnostic-section">
              <div className="panel-heading">
                <div><span className="section-eyebrow">AUTOMATIC TRIAGE</span><h2>Likely root causes</h2></div>
                <span className="value-tag">{report.rootCauseHypotheses.length} HYPOTHESES</span>
              </div>
              <p className="diagnostic-network-note">These are pattern-based hypotheses from captured evidence, not guaranteed root causes. Open the matched log lines and compare them with the actual failure.</p>
              {report.rootCauseHypotheses.map((diagnosis) => (
                <article className="diagnostic-hypothesis" key={diagnosis.code}>
                  <div className="diagnostic-hypothesis-top">
                    <strong>{diagnosis.title}</strong>
                    <span className={diagnosis.severity === "critical" ? "diagnostic-pill failed" : "diagnostic-pill warning"}>{diagnosis.confidence.toUpperCase()} CONFIDENCE</span>
                  </div>
                  <p>{diagnosis.likelyCause}</p>
                  {diagnosis.evidence.length > 0 && <details><summary>Matched log lines ({diagnosis.evidence.length})</summary><pre>{diagnosis.evidence.join("\\n")}</pre></details>}
                  <div className="diagnostic-recommendation"><b>Suggested next step:</b> {diagnosis.recommendedAction}</div>
                </article>
              ))}
            </section>
          )}

          <section className="panel diagnostic-section">
            <div className="panel-heading">
              <div><span className="section-eyebrow">PROCESS TRACE</span><h2>Recent Minecraft output</h2></div>
              <span className="value-tag">{report.recentProcessLogs.length} LINES</span>
            </div>
            <pre className="diagnostic-process-log">{report.recentProcessLogs.length ? report.recentProcessLogs.map((line) => `[${line.stream}] ${line.line}`).join("\n") : "No process output has been captured in this NOVA session."}</pre>
          </section>

          <section className="panel diagnostic-privacy">
            <div className="panel-heading"><div><span className="section-eyebrow">PRIVACY & REPORT STORAGE</span><h2>What this report contains</h2></div><ShieldCheck size={18} /></div>
            <ul>{report.privacy.map((item) => <li key={item}>{item}</li>)}</ul>
            <p>Saved locally at: <code>{report.reportPath}</code></p>
            <button className="instance-process-button" onClick={onOpenFolder}><FolderOpen size={14} /> Open report folder</button>
          </section>
        </>
      )}
    </div>
  );
}

function AccountsPage({
  playerName,
  onPlayerNameChange,
  onNavigate
}: {
  playerName: string;
  onPlayerNameChange: (value: string) => void;
  onNavigate: (page: Page) => void;
}) {
  const validName = /^[A-Za-z0-9_]{1,16}$/.test(playerName);
  return (
    <div className="nova-account-page">
      <section className="panel nova-account-panel">
        <div className="nova-account-profile">
          <div className="nova-account-avatar">{playerName.slice(0, 1).toUpperCase() || "N"}</div>
          <div>
            <span className="section-eyebrow">LOCAL PLAYER PROFILE</span>
            <h2>{playerName || "Unnamed player"}</h2>
            <p>Used by NOVA when launching Minecraft in offline mode.</p>
          </div>
        </div>
        <label className="launch-config-field nova-profile-field">
          <span>Player name</span>
          <input
            value={playerName}
            onChange={(event) => onPlayerNameChange(event.target.value.replace(/[^A-Za-z0-9_]/g, "").slice(0, 16))}
            maxLength={16}
            spellCheck={false}
            autoComplete="off"
          />
        </label>
        {!validName && <p className="nova-validation-error">Enter a Minecraft name using 1–16 letters, numbers or underscores.</p>}
        <div className="nova-account-note">
          <ShieldCheck size={17} />
          <div>
            <strong>Offline profile</strong>
            <p>Microsoft authentication is not integrated yet. This profile does not sign in to Microsoft or authorize access to online-mode servers.</p>
          </div>
        </div>
        <div className="nova-account-actions">
          <button className="button button-primary" onClick={() => onNavigate("instances")} disabled={!validName}>
            Continue to versions <ArrowRight size={15} />
          </button>
        </div>
      </section>
    </div>
  );
}

function SettingsPage({
  runtime,
  checking,
  isDesktop,
  onRefresh
}: {
  runtime: RuntimeStatus | null;
  checking: boolean;
  isDesktop: boolean;
  onRefresh: () => void;
}) {
  return (
    <div className="settings-layout">
      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <span className="section-eyebrow">SYSTEM INFORMATION</span>
            <h2>Runtime & environment</h2>
          </div>
          <button className="icon-button" onClick={onRefresh} disabled={!isDesktop || checking} aria-label="Refresh environment">
            <RefreshCw size={16} className={checking ? "spin" : ""} />
          </button>
        </div>
        <div className="settings-row">
          <div className="settings-row-icon"><Monitor size={17} /></div>
          <div className="settings-row-copy"><strong>Operating system</strong><span>Detected from the native application</span></div>
          <div className="settings-row-value">{runtime ? runtime.platform + " · " + runtime.architecture : "Unavailable"}</div>
        </div>
        <div className="settings-row">
          <div className="settings-row-icon"><TerminalSquare size={17} /></div>
          <div className="settings-row-copy"><strong>Java runtime</strong><span>{runtime?.java_version || "Java discovery status"}</span></div>
          <div className={runtime?.java_available ? "value-tag success" : "value-tag"}>
            {checking ? "Checking" : runtime?.java_available ? "Available" : runtime ? "Not found" : "Unknown"}
          </div>
        </div>
        <div className="settings-row">
          <div className="settings-row-icon"><FolderOpen size={17} /></div>
          <div className="settings-row-copy"><strong>Minecraft directory</strong><span>Default game data location</span></div>
          <div className="settings-row-value path-value">{runtime?.minecraft_directory || "Unavailable"}</div>
        </div>
      </section>

      <section className="panel settings-panel">
        <div className="panel-heading">
          <div>
            <span className="section-eyebrow">APPLICATION</span>
            <h2>About NOVA</h2>
          </div>
          <div className="about-mark">N</div>
        </div>
        <div className="about-info">
          <strong>NOVA Launcher <span className="version-chip">0.1.0 DEV</span></strong>
          <p>Independent Minecraft launcher project. The current build establishes the UI shell and native environment bridge.</p>
          <div className="about-status"><CheckCircle2 size={15} /> Foundation initialized</div>
        </div>
      </section>
    </div>
  );
}

function App() {
  const [page, setPage] = useState<Page>("overview");
  const { runtime, checking, isDesktop, refresh } = useDesktopRuntime();
  const performance = usePerformanceMetrics(isDesktop);
  const [playerName, setPlayerName] = useState(() => {
    try {
      const saved = window.localStorage.getItem("nova.profile.name");
      return saved && /^[A-Za-z0-9_]{1,16}$/.test(saved) ? saved : "NOVAPlayer";
    } catch {
      return "NOVAPlayer";
    }
  });
  const [diagnosticReport, setDiagnosticReport] = useState<DiagnosticReport | null>(null);
  const [diagnosticLoading, setDiagnosticLoading] = useState(false);
  const [diagnosticError, setDiagnosticError] = useState<string | null>(null);
  const pageInfo = PAGE_INFO[page];

  useEffect(() => {
    if (!/^[A-Za-z0-9_]{1,16}$/.test(playerName)) return;
    try {
      window.localStorage.setItem("nova.profile.name", playerName);
    } catch (cause) {
      console.warn("Unable to persist the local player name:", cause);
    }
  }, [playerName]);

  const runDiagnostics = useCallback(async (deep: boolean) => {
    if (!isDesktop || diagnosticLoading) return;
    setDiagnosticLoading(true);
    setDiagnosticError(null);
    try {
      const report = await invoke<DiagnosticReport>("run_diagnostics", {
        instanceId: null,
        deep
      });
      setDiagnosticReport(report);
    } catch (cause) {
      setDiagnosticError(String(cause));
    } finally {
      setDiagnosticLoading(false);
    }
  }, [diagnosticLoading, isDesktop]);

  const openDiagnosticFolder = useCallback(async () => {
    try {
      await invoke("open_diagnostics_folder");
    } catch (cause) {
      setDiagnosticError(String(cause));
    }
  }, []);

  useEffect(() => {
    if (!isDesktop) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<DiagnosticReport>("nova-diagnostic-report-ready", (event) => {
      setDiagnosticReport(event.payload);
      setDiagnosticError(null);
    }).then((stop) => {
      if (disposed) stop();
      else unlisten = stop;
    }).catch((cause: unknown) => {
      console.error("Unable to subscribe to diagnostic reports:", cause);
    });

    setDiagnosticLoading(true);
    void invoke<DiagnosticReport>("run_diagnostics", {
      instanceId: null,
      deep: false
    }).then((report) => {
      if (!disposed) setDiagnosticReport(report);
    }).catch((cause: unknown) => {
      if (!disposed) setDiagnosticError(String(cause));
    }).finally(() => {
      if (!disposed) setDiagnosticLoading(false);
    });

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [isDesktop]);

  return (
    <div className="app-shell">
      <main className="main-area">
        <Topbar page={page} isDesktop={isDesktop} playerName={playerName} onNavigate={setPage} />
        <div className="page-scroll">
          <div className="page-content">
            {page !== "overview" && (
              <div className="page-heading">
                <div>
                  <span className="section-eyebrow">{pageInfo.eyebrow}</span>
                  <h2>{pageInfo.title}</h2>
                  <p>{pageInfo.description}</p>
                </div>
                {page === "instances" && <div className="build-indicator"><span /> MOJANG CATALOGUE</div>}
              </div>
            )}

            {page === "overview" && <HomePage isDesktop={isDesktop} playerName={playerName} onNavigate={setPage} />}
            {page === "instances" && <InstancesPage isDesktop={isDesktop} playerName={playerName} setPlayerName={setPlayerName} />}
            {page === "accounts" && <AccountsPage playerName={playerName} onPlayerNameChange={setPlayerName} onNavigate={setPage} />}
            {page === "modpacks" && (
              <EmptyPage
                icon={Package}
                eyebrow="MODPACK DISCOVERY"
                title="Modpack discovery is not connected yet."
                description="NOVA currently focuses on a verified Vanilla installation path. Modrinth and CurseForge discovery will be added with compatibility and dependency handling."
                actionLabel="Browse versions"
                onAction={() => setPage("instances")}
              />
            )}
            {page === "mods" && (
              <EmptyPage
                icon={Puzzle}
                eyebrow="MOD LIBRARY"
                title="Mod management is not available yet."
                description="The current build does not install or manage third-party mods. This screen will be enabled after loader, dependency and rollback support is ready."
                actionLabel="View versions"
                onAction={() => setPage("instances")}
              />
            )}
            {page === "downloads" && (
              <EmptyPage
                icon={Download}
                eyebrow="DOWNLOAD MANAGER"
                title="No active downloads."
                description="Minecraft installation progress is shown in Versions while a verified installation is running."
                actionLabel="Browse versions"
                onAction={() => setPage("instances")}
              />
            )}
            {page === "performance" && (
              <PerformancePage snapshot={performance.snapshot} checking={performance.checking} isDesktop={isDesktop} onRefresh={() => void performance.refresh()} />
            )}
            {page === "diagnostics" && (
              <DiagnosticsPage
                report={diagnosticReport}
                loading={diagnosticLoading}
                error={diagnosticError}
                isDesktop={isDesktop}
                onRun={(deep) => void runDiagnostics(deep)}
                onOpenFolder={() => void openDiagnosticFolder()}
              />
            )}
            {page === "settings" && (
              <SettingsPage runtime={runtime} checking={checking} isDesktop={isDesktop} onRefresh={() => void refresh()} />
            )}

            <footer className="page-footer nova-page-footer">
              <span>NOVA <i>·</i> INDEPENDENT MINECRAFT LAUNCHER</span>
              <span>{isDesktop ? "DESKTOP RUNTIME" : "BROWSER PREVIEW"} <i className="footer-status-dot" /></span>
            </footer>
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;export default App;
