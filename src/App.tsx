import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import "./instances.css";
import {
  Activity,
  AlertCircle,
  ArrowRight,
  ArrowUpRight,
  Blocks,
  CheckCircle2,
  ChevronDown,
  Cpu,
  Download,
  FolderOpen,
  Gamepad2,
  Gauge,
  HardDrive,
  Layers3,
  LayoutDashboard,
  Monitor,
  Package,
  Play,
  Plus,
  Puzzle,
  Square,
  Terminal,
  RefreshCw,
  ScanSearch,
  Search,
  Settings2,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  UserRound,
  Wifi,
  Zap
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

type Page = "overview" | "instances" | "modpacks" | "mods" | "downloads" | "performance" | "diagnostics" | "settings";

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
  }[];
  network: { name: string; host: string; statusCode: number | null; latencyMs: number | null; reachable: boolean; detail: string }[];
  checks: DiagnosticCheck[];
  recentProcessLogs: { timestampUnix: number; stream: string; line: string }[];
  reportPath: string;
  privacy: string[];
};

const NAVIGATION: { id: Page; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: LayoutDashboard },
  { id: "instances", label: "Instances", icon: Layers3 },
  { id: "modpacks", label: "Modpacks", icon: Package },
  { id: "mods", label: "Mods", icon: Puzzle },
  { id: "downloads", label: "Downloads", icon: Download },
  { id: "performance", label: "Performance", icon: Gauge },
  { id: "diagnostics", label: "Diagnostics", icon: ScanSearch }
];

const PAGE_INFO: Record<Page, { eyebrow: string; title: string; description: string }> = {
  overview: {
    eyebrow: "YOUR MINECRAFT WORKSPACE",
    title: "Good to have you here.",
    description: "Everything you need for your next adventure, in one place."
  },
  instances: {
    eyebrow: "YOUR GAME LIBRARY",
    title: "Instances",
    description: "Keep every Minecraft version, setup and world neatly separated."
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

function Sidebar({ active, onNavigate }: { active: Page; onNavigate: (page: Page) => void }) {
  return (
    <aside className="sidebar">
      <div className="brand-lockup">
        <div className="brand-mark"><span>N</span></div>
        <div className="brand-copy">
          <strong>NOVA</strong>
          <span>LAUNCHER</span>
        </div>
        <span className="brand-version">DEV</span>
      </div>

      <div className="workspace-switcher">
        <div className="workspace-icon"><Gamepad2 size={17} /></div>
        <div className="workspace-copy">
          <span>WORKSPACE</span>
          <strong>My Minecraft</strong>
        </div>
        <ChevronDown size={15} className="muted-icon" />
      </div>

      <div className="nav-section-label">WORKSPACE</div>
      <nav className="primary-nav" aria-label="Main navigation">
        {NAVIGATION.map((item) => {
          const Icon = item.icon;
          return (
            <button
              className={active === item.id ? "nav-item active" : "nav-item"}
              key={item.id}
              onClick={() => onNavigate(item.id)}
              aria-current={active === item.id ? "page" : undefined}
            >
              <Icon size={18} strokeWidth={1.8} />
              <span>{item.label}</span>
              {item.id === "downloads" && <span className="nav-count">0</span>}
            </button>
          );
        })}
      </nav>

      <div className="sidebar-spacer" />
      <div className="sidebar-promo">
        <div className="promo-icon"><Sparkles size={16} /></div>
        <strong>Built for your next world.</strong>
        <p>A focused Minecraft experience, without the clutter.</p>
        <span className="promo-status"><span /> EARLY DEVELOPMENT</span>
      </div>

      <button
        className={active === "settings" ? "nav-item settings-link active" : "nav-item settings-link"}
        onClick={() => onNavigate("settings")}
        aria-current={active === "settings" ? "page" : undefined}
      >
        <Settings2 size={18} strokeWidth={1.8} />
        <span>Settings</span>
      </button>

      <div className="sidebar-profile">
        <div className="profile-avatar"><UserRound size={17} /></div>
        <div className="profile-copy">
          <strong>Local profile</strong>
          <span>Not signed in</span>
        </div>
        <span className="offline-dot" title="No account connected" />
      </div>
    </aside>
  );
}

function Topbar({ page, isDesktop }: { page: Page; isDesktop: boolean }) {
  return (
    <header className="topbar">
      <div className="breadcrumb">
        <span>Workspace</span>
        <span className="breadcrumb-separator">/</span>
        <strong>{PAGE_INFO[page].title}</strong>
      </div>
      <div className="topbar-actions">
        <div className={isDesktop ? "connection-chip connected" : "connection-chip"}>
          <span className="connection-dot" />
          {isDesktop ? "Desktop environment" : "Browser preview"}
        </div>
        <button className="icon-button" aria-label="Search is coming soon" title="Search is coming soon" disabled>
          <Search size={17} />
        </button>
        <div className="topbar-divider" />
        <div className="topbar-avatar">N</div>
      </div>
    </header>
  );
}

function MetricCard({
  icon: Icon,
  label,
  value,
  note,
  accent
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  note: string;
  accent: string;
}) {
  return (
    <article className="metric-card">
      <div className={"metric-icon " + accent}><Icon size={18} strokeWidth={1.9} /></div>
      <div className="metric-content">
        <span>{label}</span>
        <strong>{value}</strong>
        <small>{note}</small>
      </div>
      <div className="metric-sparkline" aria-hidden="true"><i /><i /><i /><i /><i /><i /><i /></div>
    </article>
  );
}

function Overview({
  runtime,
  checking,
  onNavigate
}: {
  runtime: RuntimeStatus | null;
  checking: boolean;
  onNavigate: (page: Page) => void;
}) {
  const javaValue = checking ? "Checking" : runtime?.java_available ? "Detected" : runtime ? "Not found" : "—";
  const javaNote = checking
    ? "Scanning local environment"
    : runtime?.java_available
      ? runtime.java_version || "Java is available"
      : runtime
        ? "Install a compatible Java runtime"
        : "Open the desktop app to scan";

  return (
    <>
      <section className="hero-panel">
        <div className="hero-copy">
          <div className="hero-kicker"><span className="kicker-line" /> THE NEXT CHAPTER STARTS HERE</div>
          <h1>Your worlds.<br /><span>Your rules.</span></h1>
          <p>A cleaner way to organize Minecraft. Build your setup, keep it in control, and get straight to playing.</p>
          <div className="hero-actions">
            <button className="button button-primary" onClick={() => onNavigate("instances")}>
              Explore instances <ArrowRight size={16} />
            </button>
            <span className="hero-footnote"><ShieldCheck size={14} /> Your game files stay yours</span>
          </div>
        </div>
        <div className="hero-art" aria-hidden="true">
          <div className="art-orbit orbit-one" />
          <div className="art-orbit orbit-two" />
          <div className="art-glow" />
          <div className="pixel-terrain terrain-back" />
          <div className="pixel-terrain terrain-front" />
          <div className="floating-cube cube-large"><span /><i /><b /></div>
          <div className="floating-cube cube-small"><span /><i /><b /></div>
          <div className="art-coordinate">NOVA / 001</div>
          <div className="art-crosshair">+</div>
        </div>
        <div className="hero-index"><span>01</span><i /> FOUNDATION BUILD</div>
      </section>

      <section className="metrics-grid" aria-label="Launcher overview">
        <MetricCard icon={Layers3} label="INSTANCES" value="0" note="No instances installed yet" accent="green" />
        <MetricCard icon={Package} label="MODPACKS" value="—" note="Discovery not connected" accent="violet" />
        <MetricCard icon={Cpu} label="JAVA RUNTIME" value={javaValue} note={javaNote} accent="blue" />
        <MetricCard icon={Activity} label="ACTIVE DOWNLOADS" value="0" note="Nothing running right now" accent="orange" />
      </section>

      <section className="dashboard-grid">
        <article className="panel recent-panel">
          <div className="panel-heading">
            <div>
              <span className="section-eyebrow">PICK UP WHERE YOU LEFT OFF</span>
              <h2>Your instances</h2>
            </div>
            <button className="text-button" onClick={() => onNavigate("instances")}>View all <ArrowUpRight size={15} /></button>
          </div>
          <div className="empty-instance">
            <div className="empty-art">
              <div className="empty-art-ring" />
              <div className="empty-art-cube"><span /><i /><b /></div>
            </div>
            <h3>A world waiting to happen.</h3>
            <p>Your installed instances will appear here. Each one gets its own game files, settings and mods.</p>
            <button className="button button-secondary" onClick={() => onNavigate("instances")}>
              <Plus size={16} /> Go to instances
            </button>
          </div>
        </article>

        <div className="right-column">
          <article className="panel quick-panel">
            <div className="panel-heading compact">
              <div>
                <span className="section-eyebrow">SHORTCUTS</span>
                <h2>Quick access</h2>
              </div>
              <Zap size={17} className="heading-muted" />
            </div>
            <button className="quick-link" onClick={() => onNavigate("modpacks")}>
              <span className="quick-icon purple"><Blocks size={17} /></span>
              <span><strong>Discover modpacks</strong><small>Explore curated experiences</small></span>
              <ArrowUpRight size={15} />
            </button>
            <button className="quick-link" onClick={() => onNavigate("mods")}>
              <span className="quick-icon lime"><Puzzle size={17} /></span>
              <span><strong>Browse mods</strong><small>Build your perfect setup</small></span>
              <ArrowUpRight size={15} />
            </button>
            <button className="quick-link" onClick={() => onNavigate("downloads")}>
              <span className="quick-icon blue"><Download size={17} /></span>
              <span><strong>Download center</strong><small>View installation activity</small></span>
              <ArrowUpRight size={15} />
            </button>
          </article>

          <article className="system-card">
            <div className="system-card-top">
              <div className="system-icon"><Monitor size={17} /></div>
              <span className="system-label">LOCAL ENVIRONMENT</span>
              <span className="system-live"><i /> LIVE</span>
            </div>
            <strong>{runtime ? runtime.platform + " · " + runtime.architecture : "Waiting for desktop runtime"}</strong>
            <p>{runtime?.minecraft_directory || "Native environment details appear when NOVA runs as a desktop application."}</p>
            <div className="system-card-bottom">
              <span><HardDrive size={13} /> Local device</span>
              <span><Wifi size={13} /> {runtime ? "Detected" : "Not connected"}</span>
            </div>
          </article>
        </div>
      </section>

      <div className="launch-bar">
        <div className="launch-context">
          <div className="launch-game-icon"><Gamepad2 size={20} /></div>
          <div>
            <strong>No instance selected</strong>
            <span>Create or install an instance to get started.</span>
          </div>
        </div>
        <button className="button launch-button" disabled title="Game installation and launching are part of the next milestone">
          <Zap size={17} fill="currentColor" /> Launch game
        </button>
      </div>
    </>
  );
}


function InstancesPage({ isDesktop }: { isDesktop: boolean }) {
  const [catalog, setCatalog] = useState<VersionCatalog | null>(null);
  const [instances, setInstances] = useState<InstalledInstance[]>([]);
  const [selectedVersion, setSelectedVersion] = useState("");
  const [includeSnapshots, setIncludeSnapshots] = useState(false);
  const [loading, setLoading] = useState(true);
  const [installing, setInstalling] = useState(false);
  const [progress, setProgress] = useState<InstallProgress | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [playerName, setPlayerName] = useState("NOVAPlayer");
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
            <p className="diagnostic-network-note">These checks send only a small HTTP request to public Minecraft endpoints. They do not transmit your diagnostic report, account details or authentication tokens.</p>
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
  const [diagnosticReport, setDiagnosticReport] = useState<DiagnosticReport | null>(null);
  const [diagnosticLoading, setDiagnosticLoading] = useState(false);
  const [diagnosticError, setDiagnosticError] = useState<string | null>(null);
  const pageInfo = PAGE_INFO[page];

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
      <Sidebar active={page} onNavigate={setPage} />
      <main className="main-area">
        <Topbar page={page} isDesktop={isDesktop} />
        <div className="page-scroll">
          <div className="page-content">
            <div className="page-heading">
              <div>
                <span className="section-eyebrow">{pageInfo.eyebrow}</span>
                <h2>{pageInfo.title}</h2>
                <p>{pageInfo.description}</p>
              </div>
              <div className="build-indicator"><span /> FOUNDATION BUILD</div>
            </div>

            {page === "overview" && <Overview runtime={runtime} checking={checking} onNavigate={setPage} />}
            {page === "instances" && <InstancesPage isDesktop={isDesktop} />}
            {page === "modpacks" && (
              <EmptyPage
                icon={Package}
                eyebrow="MODPACK DISCOVERY"
                title="A whole new way to play."
                description="Modpack discovery is not connected yet. The planned catalogue will support provider metadata, loader compatibility, dependencies and safe installation."
                actionLabel="Explore instances"
                onAction={() => setPage("instances")}
              />
            )}
            {page === "mods" && (
              <EmptyPage
                icon={Puzzle}
                eyebrow="MOD LIBRARY"
                title="Make every instance yours."
                description="The mod manager will be instance-aware, with compatibility checks, dependency resolution, version tracking and reversible updates."
                actionLabel="View instances"
                onAction={() => setPage("instances")}
              />
            )}
            {page === "downloads" && (
              <EmptyPage
                icon={Download}
                eyebrow="DOWNLOAD MANAGER"
                title="Nothing downloading."
                description="There are no active transfers. When installation is implemented, this page will show real progress, verification, retry and cancellation controls."
                actionLabel="Back to overview"
                onAction={() => setPage("overview")}
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

            <footer className="page-footer">
              <span>NOVA <i>·</i> BUILT FOR THE NEXT WORLD</span>
              <span>EARLY DEVELOPMENT <i className="footer-status-dot" /></span>
            </footer>
          </div>
        </div>
      </main>
    </div>
  );
}

export default App;
