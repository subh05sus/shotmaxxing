import { Nav } from '@/components/site/nav';
import { Footer } from '@/components/landing/more';
import { Sidebar } from '@/components/docs/sidebar';
import { getNav } from '@/lib/docs';
import { release } from '@/lib/release';

export default async function DocsLayout({ children }: { children: React.ReactNode }) {
  const [sections, r] = [await getNav(), release()];
  return (
    <>
      <Nav />
      <div className="shell grid gap-10 pt-10 pb-24 lg:grid-cols-[15rem_minmax(0,1fr)] lg:gap-12 lg:pt-14">
        <aside className="hidden lg:block">
          <Sidebar sections={sections} />
        </aside>
        <main id="main" className="min-w-0">
          {children}
        </main>
      </div>
      <Footer version={r.version} />
    </>
  );
}
