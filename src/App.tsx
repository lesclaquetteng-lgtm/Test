import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Activity,
  ArrowRight,
  ArrowUpRight,
  Blocks,
  CheckCircle2,
  ChevronDown,
  Cpu,
  Download,
  FolderOpen,
  Gamepad2,
  HardDrive,
  Layers3,
  LayoutDashboard,
  Monitor,
  Package,
  Plus,
  Puzzle,
  RefreshCw,
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

type Page = "overview" | "instances" | "modpacks" | "mods" | "downloads" | "settings";

type RuntimeStatus = {
  platform: string;
  architecture: string;
  java_available: boolean;
  java_version: string | null;
  minecraft_directory: string | null;
};

const NAVIGATION: { id: Page; label: string; icon: LucideIcon }[] = [
  { id: "overview", label: "Overview", icon: LayoutDashboard },
  { id: "instances", label: "Instances", icon: Layers3 },
  { id: "modpacks", label: "Modpacks", icon: Package },
  { id: "mods", label: "Mods", icon: Puzzle },
  { id: "downloads", label: "Downloads", icon: Download }
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
  const pageInfo = PAGE_INFO[page];

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
            {page === "instances" && (
              <EmptyPage
                icon={Layers3}
                eyebrow="INSTANCE MANAGER"
                title="Your library starts here."
                description="No instances are installed yet. The instance engine will create isolated game directories, manage versions and keep each setup independent."
                actionLabel="Back to overview"
                onAction={() => setPage("overview")}
              />
            )}
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
