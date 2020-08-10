import React from "react";
import * as Styled from "./styled";
import { Container } from "../layout";
import { Dropdown } from "../dropdown";
import { InstallScriptService, InstallScriptType } from "../../services/install-script";
import { Button } from "../button";

const service = new InstallScriptService();

export const Wizard = () => {
  const localOptions = InstallScriptService.getOptions(InstallScriptType.Local);
  const targetOptions = InstallScriptService.getOptions(InstallScriptType.Target);

  return (
    <Styled.Wizard>
      <Container>
        <Styled.Dialog>
          <Styled.StepHeader>
            <Styled.StepNumber>1</Styled.StepNumber>
            Select your environments
          </Styled.StepHeader>

          <Styled.Environments>
            <Styled.Environment>
              <h3>Local</h3>
              <p>Platform you will use to control the target.</p>

              <Styled.Dropdown>
                <Dropdown onSelect={() => {}}>
                  <option value="" data-placeholder>
                    Select
                  </option>
                  {localOptions.map((i) => (
                    <option value={i.name}>{i.name}</option>
                  ))}
                  <option value="browser">This browser</option>
                </Dropdown>
              </Styled.Dropdown>
            </Styled.Environment>
            <Styled.Separator />
            <Styled.Environment>
              <h3>Target</h3>
              <p>Platform you want to remote into.</p>
              <Styled.Dropdown>
                <Dropdown onSelect={() => {}}>
                  <option value="" data-placeholder>
                    Select
                  </option>
                  {targetOptions.map((i) => (
                    <option value={i.name}>{i.name}</option>
                  ))}
                </Dropdown>
              </Styled.Dropdown>
            </Styled.Environment>
          </Styled.Environments>
        </Styled.Dialog>

        <Styled.Dialog>
          <Styled.StepHeader>
            <Styled.StepNumber>2</Styled.StepNumber>
            <Button mode="inverted">Generate session</Button>
          </Styled.StepHeader>
        </Styled.Dialog>

        <Styled.Dialog>
          <Styled.StepHeader>
            <Styled.StepNumber>3</Styled.StepNumber>
            Run these scripts start your remote shell
          </Styled.StepHeader>

          <Styled.Environments>
            <Styled.Environment>
              <h3>Local</h3>
              <p>Platform you will use to control the target.</p>
              
            </Styled.Environment>
            <Styled.Separator />
            <Styled.Environment>
              <h3>Target</h3>
              <p>Platform you want to remote into.</p>
              
            </Styled.Environment>
          </Styled.Environments>
        </Styled.Dialog>
      </Container>
    </Styled.Wizard>
  );
};
