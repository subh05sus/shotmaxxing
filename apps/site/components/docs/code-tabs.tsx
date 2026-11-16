'use client';

/** Titled code blocks in a row (macOS | Windows), as capsule tabs over one frame at a time. */
import { Children, isValidElement } from 'react';
import { CapsuleTabs, TabPanel } from '@/components/site/capsule-tabs';

export function CodeTabs({ titles, children }: { titles: string[]; children: React.ReactNode }) {
  const blocks = Children.toArray(children).filter(isValidElement);
  return (
    <CapsuleTabs
      label="Choose one"
      size="sm"
      items={titles.map((t) => ({ value: t, label: t }))}
      listClassName="mb-3"
    >
      {blocks.map((b, i) => (
        <TabPanel key={titles[i] ?? i} value={titles[i] ?? String(i)}>
          {b}
        </TabPanel>
      ))}
    </CapsuleTabs>
  );
}
