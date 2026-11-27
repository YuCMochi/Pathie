import { Row } from '../components/Row';
import { Switch } from '../components/ui/switch';
import { update, useStore } from '../lib/store';

export function Notify() {
  const { settings } = useStore();
  return (
    <>
      <h1>提示</h1>
      <Row
        title="開啟成功時顯示提示"
        desc="Pathie 開啟路徑或網址後，在螢幕右下角短暫顯示「由 Pathie 開啟」，讓你知道剛剛跳出來的視窗是從哪來的。"
      >
        <Switch
          aria-label="開啟成功時顯示提示"
          checked={settings.showToast}
          onCheckedChange={(v) => update({ showToast: v })}
        />
      </Row>
    </>
  );
}
