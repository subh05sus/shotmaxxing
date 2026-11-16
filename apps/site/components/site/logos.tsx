import { AppleLogoIcon } from '@pikaicons/core-free';
import { Icon, type IconData } from './icon';

/** Windows' four panes, drawn in Pika's stroke style so it sits beside Pika's Apple logo. */
export const WindowsLogoIcon: IconData = [
  [
    'path',
    {
      fill: 'none',
      stroke: 'currentColor',
      strokeWidth: '2',
      strokeLinejoin: 'round',
      d: 'M3.5 5.9 10 5v6.5H3.5zM13.5 4.5 20.5 3.5v8h-7zM3.5 12.5H10V19l-6.5-.9zM13.5 12.5h7v8l-7-1z',
    },
  ],
];

export function AppleLogo({ size = 18, className }: { size?: number; className?: string }) {
  return <Icon icon={AppleLogoIcon} size={size} className={className} />;
}

export function WindowsLogo({ size = 18, className }: { size?: number; className?: string }) {
  return <Icon icon={WindowsLogoIcon} size={size} className={className} />;
}
