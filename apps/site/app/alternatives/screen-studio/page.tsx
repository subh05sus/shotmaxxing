import { ComparisonPage, pageMetadata } from '@/components/marketing/pages';
import { COMPARISONS } from '@/lib/marketing';

const page = COMPARISONS.find((c) => c.path === '/alternatives/screen-studio')!;

export const metadata = pageMetadata(page);

export default function ScreenStudioAlternative() {
  return <ComparisonPage page={page} />;
}
