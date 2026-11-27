import { openRepository, useStore } from '../lib/store';

export function About() {
  const { info } = useStore();
  return (
    <>
      <h1>關於</h1>
      <p>
        <b>Pathie</b> v{info.version}
      </p>
      <p>作者：{info.author}</p>
      <p>
        <button className="text-primary underline" onClick={openRepository}>
          {info.repository}
        </button>
      </p>
    </>
  );
}
