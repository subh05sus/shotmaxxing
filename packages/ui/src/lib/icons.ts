/**
 * Every icon in the app, named for what it means here, so changing a drawing never touches a screen.
 *
 * The free Pikaicons (https://pikaicons.com, MIT) come first, in their Stroke style. Where Pika has no drawing for an
 * idea, the free Hugeicons (https://hugeicons.com, MIT) fill in with their stroke-rounded style, as they do for the
 * drawings the owner picked (the AI mark). Both are drawn on a 24 px grid in currentColor, and `Icon` draws them at
 * one weight so they read as one set.
 */

/** An icon's drawing: SVG elements as [tag, attributes, children], the shape both sets publish. */
export type IconNode = readonly [
  tag: string,
  attrs: { readonly [name: string]: string | number },
  children?: readonly IconNode[],
];
export type IconData = readonly IconNode[];

import { AiMagicIcon } from '@hugeicons/core-free-icons';
import {
  CopyIcon,
  DeleteDustbin01Icon,
  DownloadDownIcon,
  RefreshIcon,
  SearchIcon,
} from '@pikaicons/core-free';

/** Icons that act out what they do, once, when their button is hovered (see motion.css). */
export const iconActs = new Map<IconData, 'spin' | 'tilt' | 'pop' | 'search' | 'spark'>([
  [RefreshIcon, 'spin'],
  [DeleteDustbin01Icon, 'tilt'],
  [CopyIcon, 'pop'],
  [DownloadDownIcon, 'pop'],
  [SearchIcon, 'search'],
  [AiMagicIcon, 'spark'],
]);

/** The record mark, the app's own: a solid dot in the colour of the text around it. */
export const RecordDot: IconData = [['circle', { cx: '12', cy: '12', r: '7.5', fill: 'currentColor' }]];

export {
  AlertCircleIcon as CircleAlert,
  BotIcon as Bot,
  CameraIcon as Camera,
  CheckTickCircleIcon as CircleCheck,
  ChevronDownIcon as ChevronDown,
  ChevronLeftIcon as ChevronLeft,
  ChevronRightIcon as ChevronRight,
  CopyIcon as Copy,
  DeleteDustbin01Icon as Trash,
  DownloadDownIcon as Download,
  ExternalLinkSquareIcon as ExternalLink,
  FolderIcon as FolderOpen,
  HomeIcon as House,
  InformationCircleIcon as Info,
  KeyboardWirelessIcon as Keyboard,
  LayerTwoIcon as Layers,
  LockCloseIcon as Lock,
  MicOnIcon as Mic,
  MoonIcon as Moon,
  MultipleCrossCancelIcon as X,
  MusicBeamNoteIcon as Music,
  PencilEditIcon as Pencil,
  PhotoImageIcon as Image,
  RefreshIcon as RefreshCw,
  SearchIcon as Search,
  Settings01Icon as Settings,
  ShieldCheckIcon as ShieldCheck,
  SunIcon as Sun,
  ThreeDotsMenuHorizontalIcon as Ellipsis,
  UserIcon as UserRound,
  VideoCallIcon as Video,
  VolumeTwoIcon as Volume2,
} from '@pikaicons/core-free';

// Ideas the free Pika set has no drawing for, and the owner's picks.
export {
  /** The one mark for AI: the editor's AI tool and every AI action. Never decoration. */
  AiMagicIcon as Ai,
  ArrowDataTransferVerticalIcon as ArrowDownUp,
  ArrowDownLeft01Icon as ArrowDownLeft,
  ArrowDownRight01Icon as ArrowDownRight,
  ArrowRight02Icon as ArrowRight,
  ArrowUpLeft01Icon as ArrowUpLeft,
  ArrowUpRight01Icon as ArrowUpRight,
  AudioWave02Icon as AudioLines,
  BlurIcon as Blur,
  BookOpen01Icon as BookOpen,
  BrowserIcon as AppWindow,
  ClapperboardIcon as Clapperboard,
  ClosedCaptionIcon as Captions,
  ComputerIcon as Monitor,
  ComputerVideoIcon as MonitorDot,
  CropIcon as Crop,
  Cursor02Icon as MousePointer2,
  DashboardSpeed01Icon as Gauge,
  Film01Icon as Film,
  HandIcon as Hand,
  HistoryIcon as History,
  MagicWand01Icon as Wand,
  MagicWand02Icon as WandSparkles,
  NextIcon as StepForward,
  PauseIcon as Pause,
  PlayIcon as Play,
  PreviousIcon as StepBack,
  Redo02Icon as Redo2,
  RotateLeft01Icon as RotateCcw,
  Route01Icon as Route,
  ScissorIcon as Scissors,
  SearchAddIcon as ZoomIn,
  SearchRemoveIcon as SearchX,
  StopIcon as Stop,
  TextIcon as Type,
  Tick02Icon as Check,
  Undo02Icon as Undo2,
} from '@hugeicons/core-free-icons';
