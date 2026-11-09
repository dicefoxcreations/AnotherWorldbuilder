import { api, type WorldInfo } from "../../api/client";
import { IconPicker } from "../../shared/ui/IconPicker";
import { isMobile } from "../../shared/platform";

export function InfoArea({
  world,
  onChanged,
  onError,
}: {
  world: WorldInfo;
  onChanged: () => void;
  onError: (e: unknown) => void;
}) {
  return (
    <div className="type-page">
      <h2>{world.name}</h2>
      {world.isGm && (
        <div className="line">
          <IconPicker
            value={world.icon}
            onChange={(icon) =>
              api.worldIconSet(world.id, icon).then(onChanged).catch(onError)
            }
            large
          />
          <span className="small">The world's icon — it shows in the bar and in the main menu.</span>
        </div>
      )}
      <p className="small">{world.isGm ? "You are the GM of this world." : world.supporter ? "You support this world." : "You play in this world."}</p>

      <dl className="keys wide">
        <dt>Name</dt>
        <dd>{world.name}</dd>
        <dt>Id</dt>
        <dd>
          <code className="wrap">{world.id}</code>
        </dd>
        {!isMobile && (
          <>
            <dt>Folder on this device</dt>
            <dd>
              <code className="wrap">{world.folder}</code>
            </dd>
          </>
        )}
        <dt>Articles</dt>
        <dd>{world.articleCount}</dd>
      </dl>
    </div>
  );
}
