import { Row } from '../components/Row';
import { Switch } from '../components/ui/switch';
import { setMonitoring, setStartup, useStore } from '../lib/store';

export function General() {
  const { monitoring, startup } = useStore();
  return (
    <>
      <h1>一般</h1>
      <Row title="監控剪貼簿" desc="關閉後不會偵測複製的內容。也可以左鍵點系統匣圖示快速切換。">
        <Switch aria-label="監控剪貼簿" checked={monitoring} onCheckedChange={setMonitoring} />
      </Row>
      <Row title="開機自動啟動" desc="登入 Windows 時自動在系統匣執行。">
        <Switch aria-label="開機自動啟動" checked={startup} onCheckedChange={setStartup} />
      </Row>
    </>
  );
}
